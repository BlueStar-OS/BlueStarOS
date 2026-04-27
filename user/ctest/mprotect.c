#include <unistd.h>
#include <sys/mman.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

int main() {
    long page_size = sysconf(_SC_PAGE_SIZE);
    // 分配两页，确保页对齐（mmap 返回的就是页对齐的）
    void *mem = mmap(NULL, 2 * page_size, PROT_READ | PROT_WRITE,
                     MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (mem == MAP_FAILED) {
        perror("mmap");
        return 1;
    }

    //  保护一叶
    if (mprotect(mem, 3, PROT_NONE) == -1) {
        perror("mprotect");
        return 1;
    }

    printf("i will write %p \n",(void*)mem);
    
    // 证明：尝试写第一页
    char *arr = (char*)mem;

    // 写第二页
    printf("i will write in the %p \n",(void*)arr+2);
     arr[page_size/sizeof(char)+2]='A';

    printf("I read %c \n",arr[page_size/sizeof(char)+2]);
    fflush(stdout);
    arr[0] = 'x'; // 崩溃（如果生效）
    
    // 其实第二页也会同样保护，arr[page_size] 也会崩溃。
    // 剩余的第二页后半部分也在同一页内，所以无法访问。

    // 若要更精细控制，你需要让保护范围恰好不覆盖第二个页的某个部分，但这需要 protect_len 不是从页起始开始且不覆盖整个第二页。但 mprotect 总以页为单位，即使你只保护 1.5 页，结果两页都变 PROT_NONE。

    printf("If you see this, something went wrong (should have segfaulted)\n");
    return 0;
}
