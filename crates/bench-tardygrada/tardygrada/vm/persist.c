/*
 * Tardygrada VM — Sovereign Disk Persistence
 * Direct syscalls: open, write, read, close, mkdir.
 * No malloc. No fopen. Just POSIX.
 */

#include "persist.h"
#include "context.h"   /* for tardy_agent_t */
#include "memory.h"    /* for tardy_page_read/write/lock/unlock */
#include <fcntl.h>
#include <unistd.h>
#include <sys/stat.h>
#include <string.h>
#include <stdio.h>     /* for snprintf */

static void uuid_to_hex(tardy_uuid_t id, char *buf)
{
    snprintf(buf, 33, "%016llx%016llx",
             (unsigned long long)id.hi, (unsigned long long)id.lo);
}

int tardy_persist_dump(const void *agent_ptr, const char *dir)
{
    const tardy_agent_t *a = (const tardy_agent_t *)agent_ptr;
    if (!a || a->trust != TARDY_TRUST_SOVEREIGN)
        return -1;

    /* Ensure directory exists */
    mkdir(dir, 0755);

    /* Build path */
    char path[512];
    char hex[33];
    uuid_to_hex(a->id, hex);
    snprintf(path, sizeof(path), "%s/%s.dat", dir, hex);

    /* Read value from memory */
    char data[4096];
    size_t sz = a->data_size > sizeof(data) ? sizeof(data) : a->data_size;
    tardy_page_read(&a->memory.primary, data, sz);

    /* Compute hash */
    tardy_hash_t hash;
    tardy_sha256(data, sz, &hash);

    /* Write: hash + size + data */
    int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0)
        return -1;

    ssize_t w1 = write(fd, &hash, sizeof(hash));
    ssize_t w2 = write(fd, &sz, sizeof(sz));
    ssize_t w3 = write(fd, data, sz);
    close(fd);

    if (w1 != (ssize_t)sizeof(hash) ||
        w2 != (ssize_t)sizeof(sz) ||
        w3 != (ssize_t)sz)
        return -1;

    return 0;
}

int tardy_persist_load(void *agent_ptr, const char *dir)
{
    tardy_agent_t *a = (tardy_agent_t *)agent_ptr;
    if (!a)
        return -1;

    char path[512];
    char hex[33];
    uuid_to_hex(a->id, hex);
    snprintf(path, sizeof(path), "%s/%s.dat", dir, hex);

    int fd = open(path, O_RDONLY);
    if (fd < 0)
        return -1;

    tardy_hash_t stored_hash;
    size_t sz;
    char data[4096];

    if (read(fd, &stored_hash, sizeof(stored_hash)) != (ssize_t)sizeof(stored_hash)) {
        close(fd);
        return -1;
    }
    if (read(fd, &sz, sizeof(sz)) != (ssize_t)sizeof(sz)) {
        close(fd);
        return -1;
    }
    if (sz > sizeof(data)) {
        close(fd);
        return -1;
    }
    if (read(fd, data, sz) != (ssize_t)sz) {
        close(fd);
        return -1;
    }
    close(fd);

    /* Verify hash */
    tardy_hash_t check;
    tardy_sha256(data, sz, &check);
    if (!tardy_hash_eq(&check, &stored_hash))
        return -1;

    /* Write back to agent memory (if memory exists) */
    if (a->memory.primary.ptr) {
        tardy_page_unlock(&a->memory.primary);
        tardy_page_write(&a->memory.primary, data, sz);
        tardy_page_lock(&a->memory.primary);
    }

    return 0;
}

int tardy_persist_exists(tardy_uuid_t id, const char *dir)
{
    char path[512];
    char hex[33];
    uuid_to_hex(id, hex);
    snprintf(path, sizeof(path), "%s/%s.dat", dir, hex);
    return access(path, F_OK) == 0 ? 1 : 0;
}
