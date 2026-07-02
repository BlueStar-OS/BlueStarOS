#include <string.h>
#include <unistd.h>

#include "testlib.h"

int main(void) {
    const char msg[] = "[TESTOS][WRITE] hello from write syscall test\n";
    ssize_t written = write(STDOUT_FILENO, msg, strlen(msg));
    TEST_ASSERT_EQ(written, (ssize_t)strlen(msg));
    TEST_PASS();
}
