#include <sys/types.h>
#include <unistd.h>

#include "testlib.h"

int main(void) {
    pid_t pid = getpid();
    TEST_LOG("getpid returned %ld", (long)pid);
    TEST_ASSERT(pid > 0);
    TEST_PASS();
}
