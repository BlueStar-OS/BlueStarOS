// 模拟内核elf解析没有处理权限合并，导致紧凑段权限错误挂断


#include <stdio.h>




void hello() {
    printf("Hello\n");
}

// 一个全局变量，会被放在 .data 段
// 我们不仅要读它，还要写它，触发 Write Fault
int evil_var = 123;

void change(){
    // 如果它和 hello() 在同一页，且该页被映射为R-X，这里必挂
    evil_var = 456; 
}

int main() {
    printf("Function addr: %p\n", hello);
    printf("Variable addr: %p\n", &evil_var);
    
    change();
    
    printf("Modified var: %d\n", evil_var);
    return 0;
}