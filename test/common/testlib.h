#ifndef BLUESTAROS_TESTLIB_H
#define BLUESTAROS_TESTLIB_H

#include <stdio.h>
#include <stdlib.h>

#define TEST_LOG(fmt, ...) \
    do { printf("[TESTOS][LOG] " fmt "\n", ##__VA_ARGS__); } while (0)

#define TEST_FAIL(fmt, ...)                                                    \
    do {                                                                       \
        printf("[TESTOS][FAIL] %s:%d: " fmt "\n", __FILE__, __LINE__,        \
               ##__VA_ARGS__);                                                 \
        return 1;                                                              \
    } while (0)

#define TEST_ASSERT(expr)                                                       \
    do {                                                                       \
        if (!(expr)) {                                                          \
            TEST_FAIL("assertion failed: %s", #expr);                          \
        }                                                                      \
    } while (0)

#define TEST_ASSERT_EQ(actual, expected)                                        \
    do {                                                                       \
        long _test_actual = (long)(actual);                                     \
        long _test_expected = (long)(expected);                                 \
        if (_test_actual != _test_expected) {                                   \
            TEST_FAIL("assertion failed: %s == %s (got %ld, expected %ld)",    \
                      #actual, #expected, _test_actual, _test_expected);        \
        }                                                                      \
    } while (0)

#define TEST_PASS()                                                            \
    do {                                                                       \
        printf("[TESTOS][PASS] %s\n", __FILE__);                              \
        return 0;                                                              \
    } while (0)

#endif
