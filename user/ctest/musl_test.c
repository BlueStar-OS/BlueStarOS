/*
 * BlueStarOS Musl Libc Compatibility Test Suite
 * 
 * 编译方法: 
 * riscv64-linux-musl-gcc -static musl_test.c -o musl_test
 * 
 * 注意：必须使用 -static，因为你的 OS 暂时还不支持动态链接器 (ld-linux.so)
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <fcntl.h>
#include <sys/wait.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/mman.h>
#include <dirent.h>
#include <time.h>
#include <errno.h>

// 简单的断言宏
#define TEST_START(name) printf("\n[TEST] === %s ===\n", name)
#define ASSERT(cond, msg) do { \
    if (!(cond)) { \
        printf("[FAIL] %s (Line %d)\n", msg, __LINE__); \
        exit(1); \
    } else { \
        printf("[PASS] %s\n", msg); \
    } \
} while(0)

#define ASSERT_OK(ret, msg) ASSERT((ret) >= 0, msg)

/* 1. 基础 I/O 和 文件系统测试 */
void test_file_io() {
    TEST_START("File I/O (fopen/fwrite/fread)");
    
    const char *filename = "test_musl.txt";
    const char *content = "Hello BlueStarOS with Musl!";
    char buf[100];

    // 写文件
    FILE *fp = fopen(filename, "w+");
    ASSERT(fp != NULL, "fopen w+");
    
    size_t wrote = fwrite(content, 1, strlen(content), fp);
    ASSERT(wrote == strlen(content), "fwrite count");
    
    // 移动指针
    int seek_ret = fseek(fp, 0, SEEK_SET);
    ASSERT(seek_ret == 0, "fseek");

    // 读文件
    size_t read_cnt = fread(buf, 1, sizeof(buf), fp);
    ASSERT(read_cnt == strlen(content), "fread count");
    buf[read_cnt] = '\0';
    
    ASSERT(strcmp(buf, content) == 0, "content verify");
    printf("    Read back: %s\n", buf);

    fclose(fp);

    // Unlink
    int ret = unlink(filename);
    ASSERT_OK(ret, "unlink");
}

/* 2. 内存管理测试 (Malloc/Mmap) */
void test_memory() {
    TEST_START("Memory (malloc/free/mmap)");

    // 测试 1: 小内存 (通常通过 brk 分配)
    printf("    Testing small malloc...\n");
    int *arr = (int*)malloc(100 * sizeof(int));
    ASSERT(arr != NULL, "malloc small");
    for(int i=0; i<100; i++) arr[i] = i;
    int sum = 0;
    for(int i=0; i<100; i++) sum += arr[i];
    ASSERT(sum == 4950, "small malloc data integrity");
    free(arr);

    // 测试 2: 大内存 (通常通过 mmap 分配)
    // musl 对于大块内存会直接调用 mmap
    printf("    Testing large malloc (1MB)...\n");
    size_t large_size = 1024 * 1024;
    char *large_buf = (char*)malloc(large_size);
    ASSERT(large_buf != NULL, "malloc large");
    // 写入两端，触发缺页
    large_buf[0] = 'A';
    large_buf[large_size - 1] = 'Z';
    ASSERT(large_buf[0] == 'A', "large buf access head");
    ASSERT(large_buf[large_size - 1] == 'Z', "large buf access tail");
    free(large_buf);

    // 测试 3: 显式 mmap
    printf("    Testing explicit mmap...\n");
    void *map_ptr = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    ASSERT(map_ptr != MAP_FAILED, "mmap anonymous");
    *(int*)map_ptr = 12345;
    ASSERT(*(int*)map_ptr == 12345, "mmap read/write");
    munmap(map_ptr, 4096);
}

/* 3. 进程管理测试 (Fork/Exec/Wait) */
void test_process() {
    TEST_START("Process (fork/waitpid)");

    printf("    Parent pid: %d\n", getpid());
    
    int pid = fork();
    ASSERT(pid >= 0, "fork");

    if (pid == 0) {
        // Child
        printf("    [Child] Hello from child! pid=%d, ppid=%d\n", getpid(), getppid());
        // 测试 heap 在 copy-on-write 后是否正常
        char *p = malloc(10);
        *p = 'C';
        exit(42);
    } else {
        // Parent
        int status;
        int ret = waitpid(pid, &status, 0);
        ASSERT(ret == pid, "waitpid return value");
        
        // Musl 的 WEXITSTATUS 宏依赖正确的 status 布局
        if (WIFEXITED(status)) {
            int code = WEXITSTATUS(status);
            printf("    [Parent] Child exited with code: %d\n", code);
            ASSERT(code == 42, "child exit code");
        } else {
            ASSERT(0, "Child did not exit normally");
        }
    }
}

/* 4. 目录操作测试 */
void test_directory() {
    TEST_START("Directory (mkdir/opendir/readdir)");

    const char *dirname = "musl_test_dir";
    
    // mkdir
    int ret = mkdir(dirname, 0755);
    // 允许已存在
    if (ret < 0 && errno != EEXIST) {
        ASSERT_OK(ret, "mkdir");
    }

    // opendir
    DIR *d = opendir(".");
    ASSERT(d != NULL, "opendir '.'");

    struct dirent *dir;
    int found = 0;
    printf("    Listing files:\n");
    while ((dir = readdir(d)) != NULL) {
        printf("      - %s\n", dir->d_name);
        if (strcmp(dir->d_name, dirname) == 0) {
            found = 1;
        }
    }
    ASSERT(found, "readdir found created dir");
    closedir(d);

    // rmdir (using unlink logic in your kernel maybe?)
    // libc rmdir usually calls unlinkat with AT_REMOVEDIR
    // If you haven't implemented sys_unlinkat with flag, skip this.
}

/* 5. 管道测试 */
void test_pipe() {
    TEST_START("Pipe (pipe/read/write)");

    int fds[2];
    int ret = pipe(fds);
    ASSERT_OK(ret, "pipe creation");

    int pid = fork();
    if (pid == 0) {
        // Child: write
        close(fds[0]);
        const char *msg = "Pipe Data from Child";
        write(fds[1], msg, strlen(msg));
        close(fds[1]);
        exit(0);
    } else {
        // Parent: read
        close(fds[1]);
        char buf[64] = {0};
        int n = read(fds[0], buf, sizeof(buf));
        ASSERT(n > 0, "read from pipe");
        printf("    Received: %s\n", buf);
        ASSERT(strcmp(buf, "Pipe Data from Child") == 0, "pipe data verify");
        close(fds[0]);
        wait(NULL);
    }
}

/* 6. 时间测试 */
void test_time() {
    TEST_START("Time (gettimeofday/time/sleep)");

    struct timeval start, end;
    gettimeofday(&start, NULL);
    printf("    Start time: %ld.%ld\n", start.tv_sec, start.tv_usec);

    ASSERT(start.tv_sec > 1000, "Time seems sane (not 0)");

    // Sleep 100ms
    printf("    Sleeping 100ms...\n");
    usleep(100000); // requires nanosleep syscall usually

    gettimeofday(&end, NULL);
    long diff_ms = (end.tv_sec - start.tv_sec) * 1000 + (end.tv_usec - start.tv_usec) / 1000;
    
    printf("    End time: %ld.%ld\n", end.tv_sec, end.tv_usec);
    printf("    Sleep duration: %ld ms\n", diff_ms);
    
    // 允许误差
    ASSERT(diff_ms >= 90, "sleep duration >= 90ms");
}

int main(int argc, char *argv[]) {
    printf("\n");
    printf("**********************************************\n");
    printf("* BlueStarOS Musl Compatibility Verification *\n");
    printf("**********************************************\n");
    printf("Args: argc=%d, argv[0]=%s\n", argc, argv[0]);

    // 顺序执行所有测试
    test_memory();   // 先测内存，它是基石
    test_file_io();  // 测文件系统基础
    test_process();  // 测进程
    test_pipe();     // 测 IPC
    test_time();     // 测时间
    test_directory(); // 测目录

    printf("\n[SUCCESS] All Musl tests passed successfully!\n");
    return 0;
}