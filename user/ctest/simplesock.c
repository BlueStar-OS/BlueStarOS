/*
 * simplesock.c — UDP socket 测试程序
 *
 * 功能: 创建 UDP socket → bind 端口 → 发送 "hello" → 接收回复
 * 用法: simplesock [loop]
 *   传入 loop 参数后会持续循环发送 UDP 包 (Ctrl+C 退出)
 *
 * 对应内核 syscall 链路:
 *   SYS_SOCKET(198)  → sys_socket()
 *   SYS_BIND(200)    → sys_bind()
 *   SYS_SENDTO(206)  → sys_sendto()
 *   SYS_RECVFROM(207)→ sys_recvfrom()
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>

#define BIND_PORT    8080
#define DEST_PORT    9090
#define DEST_IP      "10.0.0.1"
#define BUF_SIZE     128

int main(int argc, char *argv[])
{
    int fd;
    int do_loop = 0;
    struct sockaddr_in local_addr;
    struct sockaddr_in dest_addr;
    char buf[BUF_SIZE];
    char msg[64];
    ssize_t n;
    unsigned int seq = 0;

    if (argc > 1 && strcmp(argv[1], "loop") == 0)
        do_loop = 1;

    /* ─── 1. 创建 UDP socket ─── */
    fd = socket(AF_INET, SOCK_DGRAM, 0);
    if (fd < 0) {
        perror("socket");
        return 1;
    }
    printf("[+] socket created, fd = %d\n", fd);

    /* ─── 2. bind 本地端口 ─── */
    memset(&local_addr, 0, sizeof(local_addr));
    local_addr.sin_family      = AF_INET;
    local_addr.sin_port        = htons(BIND_PORT);
    local_addr.sin_addr.s_addr = htonl(INADDR_ANY);

    if (bind(fd, (struct sockaddr *)&local_addr, sizeof(local_addr)) < 0) {
        perror("bind");
        close(fd);
        return 1;
    }
    printf("[+] bound to port %d\n", BIND_PORT);

    /* ─── 3. 准备目标地址 ─── */
    memset(&dest_addr, 0, sizeof(dest_addr));
    dest_addr.sin_family      = AF_INET;
    dest_addr.sin_port        = htons(DEST_PORT);
    inet_pton(AF_INET, DEST_IP, &dest_addr.sin_addr);

    /* ─── 4. 发送: 单次 或 循环 ─── */
    do {
        if (do_loop)
            snprintf(msg, sizeof(msg), "hello #%u", seq++);
        else
            snprintf(msg, sizeof(msg), "hello");

        n = sendto(fd, msg, strlen(msg), 0,
                   (struct sockaddr *)&dest_addr, sizeof(dest_addr));
        if (n < 0) {
            perror("sendto");
            break;
        }
        printf("[+] sent %zd bytes to %s:%d: \"%s\"\n",
               n, DEST_IP, DEST_PORT, msg);

        if (!do_loop) {
            /* 单次模式: 等一下回复 */
            printf("[*] waiting for reply...\n");
            n = recvfrom(fd, buf, BUF_SIZE - 1, 0, NULL, NULL);
            if (n > 0) {
                buf[n] = '\0';
                printf("[+] received %zd bytes: \"%s\"\n", n, buf);
            } else {
                printf("[-] no reply (timeout or no peer)\n");
            }
        } else {
            /* 循环模式: 每 500ms 发一包 */
            usleep(500000);
        }
    } while (do_loop);

    close(fd);
    printf("[+] socket closed, done.\n");
    return 0;
}
