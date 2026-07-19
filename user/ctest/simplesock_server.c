/*
 * simplesock_server.c — UDP 服务端
 *
 * 功能: bind 10.0.0.1:9090 → recvfrom 阻塞等待 → 回复到 10.0.0.2:8080
 * 用法: simplesock_server [loop]
 *   传入 loop 参数后持续循环收发 (Ctrl+C 退出)
 *
 * 配合 simplesock.c (客户端) 使用:
 *   客户端: 0.0.0.0:8080 → sendto 10.0.0.1:9090 "hello"
 *   服务端: 10.0.0.1:9090 → recvfrom → sendto 10.0.0.2:8080 "world"
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>

#define LISTEN_IP    "10.0.0.1"
#define LISTEN_PORT  9090
#define REPLY_IP     "10.0.0.2"
#define REPLY_PORT   8080
#define BUF_SIZE     128

int main(int argc, char *argv[])
{
    int fd;
    int do_loop = 0;
    struct sockaddr_in listen_addr;
    struct sockaddr_in peer_addr;
    struct sockaddr_in reply_addr;
    socklen_t peer_len;
    char buf[BUF_SIZE];
    char reply[64];
    ssize_t n;
    unsigned int count = 0;

    if (argc > 1 && strcmp(argv[1], "loop") == 0)
        do_loop = 1;

    /* ─── 1. 创建 UDP socket ─── */
    fd = socket(AF_INET, SOCK_DGRAM, 0);
    if (fd < 0) {
        perror("socket");
        return 1;
    }
    printf("[+] socket created, fd = %d\n", fd);

    /* ─── 2. bind 到 10.0.0.1:9090 ─── */
    memset(&listen_addr, 0, sizeof(listen_addr));
    listen_addr.sin_family      = AF_INET;
    listen_addr.sin_port        = htons(LISTEN_PORT);
    inet_pton(AF_INET, LISTEN_IP, &listen_addr.sin_addr);

    if (bind(fd, (struct sockaddr *)&listen_addr, sizeof(listen_addr)) < 0) {
        perror("bind");
        close(fd);
        return 1;
    }
    printf("[+] bound to %s:%d\n", LISTEN_IP, LISTEN_PORT);

    /* ─── 3. 准备回复地址 (固定) ─── */
    memset(&reply_addr, 0, sizeof(reply_addr));
    reply_addr.sin_family      = AF_INET;
    reply_addr.sin_port        = htons(REPLY_PORT);
    inet_pton(AF_INET, REPLY_IP, &reply_addr.sin_addr);

    /* ─── 4. 收发: 单次 或 循环 ─── */
    do {
        if (do_loop) {
            snprintf(reply, sizeof(reply), "world #%u", count++);
        } else {
            printf("[*] waiting for incoming packet...\n");
            peer_len = sizeof(peer_addr);
            n = recvfrom(fd, buf, BUF_SIZE - 1, 0,
                         (struct sockaddr *)&peer_addr, &peer_len);
            if (n < 0) {
                perror("recvfrom");
                break;
            }
            buf[n] = '\0';
            printf("[+] received %zd bytes from %s:%d: \"%s\"\n",
                   n,
                   inet_ntoa(peer_addr.sin_addr),
                   ntohs(peer_addr.sin_port),
                   buf);
            snprintf(reply, sizeof(reply), "world");
        }

        n = sendto(fd, reply, strlen(reply), 0,
                   (struct sockaddr *)&reply_addr, sizeof(reply_addr));
        if (n < 0) {
            perror("sendto");
            break;
        }
        printf("[+] sent %zd bytes to %s:%d: \"%s\"\n",
               n, REPLY_IP, REPLY_PORT, reply);

        if (do_loop)
            usleep(500000);
    } while (do_loop);

    close(fd);
    printf("[+] socket closed, done.\n");
    return 0;
}
