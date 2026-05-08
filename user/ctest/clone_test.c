#define _GNU_SOURCE    // 必须定义这个，才能使用 glibc 提供的 clone() 包装器
#include <sched.h>     // clone() 相关的 flags
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <sys/types.h>
#include <signal.h>

// 为子进程分配 1MB 的栈空间
#define STACK_SIZE (1024 * 1024)

/* 
 * 这是一个全局变量（位于 .data 或 .bss 段）。
 * 如果是传统的 fork()，父子进程有各自的物理页副本（写时复制），修改互不影响。
 * 如果是带了 CLONE_VM 的 clone()，它们将指向同一块物理内存。
 */
volatile int shared_var = 100;

/*
 * 这是子进程（线程）的入口函数。
 * 类似于 pthread_create 里的入口函数。
 */
int child_func(void *arg) {
    printf("\n>>> [子进程] 启动！TID/PID = %d\n", getpid());
    printf(">>> [子进程] 收到了父进程的留言: %s\n", (char *)arg);
    printf(">>> [子进程] 读取共享变量初始值: %d\n", shared_var);

    // 核心动作：修改全局变量
    shared_var = 200;
    printf(">>> [子进程] 已将共享变量修改为: %d\n", shared_var);
    
    printf(">>> [子进程] 执行完毕，准备退出...\n");
    return 0; // 返回 0 相当于调用 exit(0)
}

int main() {
    printf("[父进程] 启动！PID = %d\n", getpid());
    printf("[父进程] 共享变量初始值: %d\n", shared_var);

    /* 
     * 1. 为子进程准备独立的栈。
     * 因为如果带了 CLONE_VM，父子同处一个地址空间。
     * 如果不用新栈，子进程和父进程的局部变量会互相踩踏（Stack Corruption）。
     */
    char *stack = malloc(STACK_SIZE);
    if (stack == NULL) {
        perror("malloc 栈分配失败");
        exit(EXIT_FAILURE);
    }

    /* 
     * 注意：RISC-V/x86 的栈都是“向下生长”（从高地址往低地址扩展）。
     * 所以传给 clone 的必须是这块内存的最高地址（栈底）！
     */
    char *stack_top = stack + STACK_SIZE;

    /*
     * 2. 准备 clone 的核心“菜单” (Flags)
     * - CLONE_VM:  与父进程共享内存描述符 (mm_struct / MemorySet)
     * - SIGCHLD:   极其重要！这告诉内核，子进程死后给父进程发 SIGCHLD 信号。
     *              如果不加这个，父进程的 waitpid() 会等不到子进程！
     */
    int clone_flags = CLONE_VM | SIGCHLD;

    char *arg = "Hello from BlueStarOS Parent!";

    printf("[父进程] 正在调用 clone() 创造新生命...\n");

    /*
     * 3. 发起 clone 系统调用
     * glibc 的 clone 包装器原型: 
     * int clone(int (*fn)(void *), void *stack, int flags, void *arg, ...);
     */
    pid_t child_pid = clone(child_func, stack_top, clone_flags, arg);

    if (child_pid == -1) {
        perror("clone 调用失败");
        free(stack);
        exit(EXIT_FAILURE);
    }

    printf("[父进程] clone() 成功！创建了子进程 PID = %d\n", child_pid);

    /*
     * 4. 阻塞等待子进程结束收尸
     */
    int status;
    printf("[父进程] 进入 waitpid() 等待子进程退出...\n");
    if (waitpid(child_pid, &status, 0) == -1) {
        perror("waitpid 失败");
    } else {
        if (WIFEXITED(status)) {
            printf("\n[父进程] 收到子进程退出信号，退出码: %d\n", WEXITSTATUS(status));
        }
    }

    /*
     * 5. 见证奇迹的时刻：检查全局变量！
     */
    printf("[父进程] 醒来，检查共享变量的值: %d\n", shared_var);
    if (shared_var == 200) {
        printf("🎉 [测试结果] SUCCESS！你看到了 %d，说明 CLONE_VM 完美生效，父子进程成功共享了页表！\n", shared_var);
    } else {
        printf("❌ [测试结果] FAILED！变量依然是 %d，你的内核把它当成普通的 fork 运行了（深拷贝了内存）！\n", shared_var);
    }

    // 释放栈内存，文明退出
    free(stack);
    return 0;
}