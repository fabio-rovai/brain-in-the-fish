/*
 * Tardygrada VM — Minimal Crypto
 * SHA-256 and ed25519 helpers. No dependencies.
 */

#ifndef TARDY_CRYPTO_H
#define TARDY_CRYPTO_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

/* SHA-256 hash */
typedef struct {
    uint8_t bytes[32];
} tardy_hash_t;

/* ed25519 key pair */
typedef struct {
    uint8_t secret[64];
    uint8_t public[32];
} tardy_keypair_t;

/* ed25519 signature */
typedef struct {
    uint8_t bytes[64];
} tardy_signature_t;

/* Hash arbitrary data */
void tardy_sha256(const void *data, size_t len, tardy_hash_t *out);

/* Compare two hashes — constant time */
bool tardy_hash_eq(const tardy_hash_t *a, const tardy_hash_t *b);

/* Generate key pair */
void tardy_keygen(tardy_keypair_t *kp);

/* Sign data */
void tardy_sign(const tardy_keypair_t *kp, const void *data, size_t len,
                tardy_signature_t *out);

/* Verify signature */
bool tardy_verify(const uint8_t pub[32], const void *data, size_t len,
                  const tardy_signature_t *sig);

#endif /* TARDY_CRYPTO_H */
