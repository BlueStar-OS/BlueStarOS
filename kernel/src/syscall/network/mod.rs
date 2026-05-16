// sys_socket: 创建一个内核 Socket 结构体，返回一个 fd。

// sys_bind: 把这个 fd 绑定到指定的端口（比如 8080）。极其关键！ 只有 bind 了，内核才知道收到的包该交给哪个 Socket。

// sys_sendto: 用户态传入一段内存（比如 "Hello"）和目标 IP/Port，你的内核把它们组装成 UDP 包，调用你写的 e1000_transmit 发出去。

// sys_recvfrom: 用户态阻塞等待。内核收到 UDP 包后，唤醒这个进程，把包的数据拷贝到用户态的 Buffer 里。