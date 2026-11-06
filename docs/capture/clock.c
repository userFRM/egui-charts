/* Capture UI clocks advance with the script. Only the main thread's UI pass uses
 * virtual monotonic time; rendering and background thread deadlines stay real. */
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

int clock_gettime(clockid_t clock, struct timespec *time) {
    const char *path = getenv("CAPTURE_CLOCK");
    if (path && (clock == CLOCK_REALTIME ||
                 (clock == CLOCK_MONOTONIC && syscall(SYS_gettid) == getpid()))) {
        FILE *file = fopen(path, "r");
        if (file) {
            long long seconds;
            long nanos;
            long long monotonic = -1;
            int count = fscanf(file, "%lld %ld %lld", &seconds, &nanos, &monotonic);
            fclose(file);
            if (clock == CLOCK_MONOTONIC && count == 3 && monotonic >= 0) {
                time->tv_sec = monotonic / 1000000000LL;
                time->tv_nsec = monotonic % 1000000000LL;
                return 0;
            }
            if (clock == CLOCK_REALTIME && count >= 2 && nanos >= 0 && nanos < 1000000000L) {
                time->tv_sec = seconds;
                time->tv_nsec = nanos;
                return 0;
            }
        }
    }
    return syscall(SYS_clock_gettime, clock, time);
}
