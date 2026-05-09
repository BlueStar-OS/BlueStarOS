#define _GNU_SOURCE // 必须定义，否则 sched.h 不会暴露 clone 的原型
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <string.h>

// 定义分配给子进程的栈大小 (1MB)
#define STACK_SIZE (1024 * 1024)

// 这是一个全局变量，用来测试 CLONE_VM (共享内存) 的语义
volatile int shared_var = 0;

/*
 * 子进程执行的函数
 * 注意：当使用 CLONE_VM 时，因为没有设置完整的 pthread TLS(线程局部存储)，
 * 在这里面调用 printf 或 malloc 等可能包含内部锁的 libc 函数是有死锁风险的。
 * 所以我们使用最底层的 async-signal-safe 函数 write()。
 */
int child_func(void *arg) {
    int mode = *(int *)arg;
    char msg[64];
    int len;

    if (mode == 1) {
        // 模式 1: 带有 CLONE_VM，修改全局变量
        shared_var = 42;
        len = snprintf(msg, sizeof(msg), "[Child] CLONE_VM active. Set shared_var = 42\n");
    } else {
        // 模式 0: 不带 CLONE_VM，修改的是子进程独立地址空间中的副本
        shared_var = 100;
        len = snprintf(msg, sizeof(msg), "[Child] No CLONE_VM. Set my copied shared_var = 100\n");
    }

    // 使用 write 直接输出，避免 stdio 缓冲区的锁竞争问题
    write(STDOUT_FILENO, msg, len);

    // 返回值相当于传给 _exit() 的退出状态码
    return 0;
}

/*
 * 封装好的 clone 测试启动器
 */
void run_clone_test(int use_clone_vm) {
    char *stack;
    char *stack_top;
    pid_t pid;
    int mode = use_clone_vm;

    // 1. 分配子进程的栈
    // 必须使用 malloc/mmap 在堆上分配，不能使用主函数的局部变量数组，
    // 因为主函数返回或被破坏会导致子进程栈崩溃。
    stack = malloc(STACK_SIZE);
    if (!stack) {
        perror("malloc stack failed");
        exit(EXIT_FAILURE);
    }

    // 2. 计算栈顶指针
    // 绝大多数架构（x86, x86_64, ARM）的栈都是向下生长（从高地址向低地址），
    // 所以传给 clone 的 stack 参数必须是分配内存的最高地址。
    stack_top = stack + STACK_SIZE;

    // 3. 准备 clone 的标志位 (flags)
    // SIGCHLD: 极其重要！如果不加这个标志，子进程退出时不会给父进程发送信号，
    // waitpid 将永远等不到子进程退出，子进程会变成游离的僵尸进程。
    int flags = SIGCHLD;
    if (use_clone_vm) {
        flags |= CLONE_VM; // 共享虚拟内存空间
    }

    printf("\n--- Starting test (CLONE_VM = %d) ---\n", use_clone_vm);
    printf("[Parent] Before clone, shared_var = %d\n", shared_var);

    // 4. 发起 clone 系统调用
    // 参数: 函数指针, 栈顶指针, 标志位, 传给函数的参数
    pid = clone(child_func, stack_top, flags, &mode);
    
    if (pid == -1) {
        perror("clone failed");
        free(stack);
        exit(EXIT_FAILURE);
    }

    // 5. 等待子进程退出
    int status;
    if (waitpid(pid, &status, 0) == -1) {
        perror("waitpid failed");
    }

    // 6. 检查全局变量的值，验证内存共享语义
    printf("[Parent] Child (PID: %d) exited. Now shared_var = %d\n", pid, shared_var);

    // 释放栈内存
    free(stack);
}

int main() {
    printf("Musl libc clone() semantics test build.\n");

    // 测试 1: 不使用 CLONE_VM (类似 fork 语义)
    // 父子进程有独立的地址空间，子进程对 shared_var 的修改对父进程不可见。
    shared_var = 0; 
    run_clone_test(0);

    // 测试 2: 使用 CLONE_VM (类似 pthread 语义)
    // 父子进程共享地址空间，子进程对 shared_var 的修改会直接反映到父进程中。
    shared_var = 0; 
    run_clone_test(1);

    return 0;
}