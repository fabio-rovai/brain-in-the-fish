/*
 * Tardygrada — Utility helpers
 */

#ifndef TARDY_UTIL_H
#define TARDY_UTIL_H

#include <unistd.h>
#include <string.h>

/* Suppress GCC warn_unused_result for write() */
static inline void tardy_write(int fd, const void *buf, size_t len)
{
    ssize_t r = write(fd, buf, len);
    (void)r;
}

static inline void tardy_print(const char *s)
{
    tardy_write(STDOUT_FILENO, s, strlen(s));
}

static inline void tardy_eprint(const char *s)
{
    tardy_write(STDERR_FILENO, s, strlen(s));
}

#endif /* TARDY_UTIL_H */
