#include<netinet/in.h>
#include<stdio.h>
#include<stdlib.h>
#include<sys/socket.h>
#include<string.h>
#include<unistd.h>
#include <arpa/inet.h>



#define BIND_PORT    8080
#define DEST_PORT    9090
#define DEST_IP      "10.0.0.1"


int main(){
    int fd;
    fd = socket(AF_INET,SOCK_DGRAM,IPPROTO_UDP);
    if (fd<0)
    {
        perror("socket");
        return 1;
    }
    printf("[+] socket created, fd = %d\n", fd);

    struct sockaddr_in local_addr;
    struct sockaddr_in dest_addr;
    char buf[60];
    char msg[64];

    memset(&local_addr,0,sizeof(local_addr));
    memset(&dest_addr,0,sizeof(dest_addr));

    local_addr.sin_family = AF_INET;
    local_addr.sin_port = htons(BIND_PORT);
    local_addr.sin_addr.s_addr = htonl(INADDR_ANY);
    
    if (bind(fd,(struct sockaddr*)&local_addr,sizeof(local_addr)) < 0)
    {
        perror("bind");
        close(fd);
        return 1;
        /* code */
    }
    printf("[+] bound to port %d\n", BIND_PORT);
    
    /* ─── 3. 准备目标地址 ─── */
    memset(&dest_addr, 0, sizeof(dest_addr));
    dest_addr.sin_family      = AF_INET;
    dest_addr.sin_port        = htons(DEST_PORT);
    inet_pton(AF_INET, DEST_IP, &dest_addr.sin_addr);
    snprintf(msg,sizeof(msg),"hello world!");
    int n = sendto(fd, msg, strlen(msg), 0,
                (struct sockaddr *)&dest_addr, sizeof(dest_addr));
    if (n < 0) {
        perror("sendto");
    }
    printf("[+] sent %zd bytes to %s:%d: \"%s\"\n",
            n, DEST_IP, DEST_PORT, msg);
    close(fd);
    return 0;
}