#ifndef TARDY_DECOMPOSE_H
#define TARDY_DECOMPOSE_H

#include "pipeline.h"

/* Decompose free text into RDF-like triples using pattern matching.
 * No LLM needed — simple English sentence patterns.
 * Returns number of triples extracted. */
int tardy_decompose(const char *text, int len,
                     tardy_triple_t *triples, int max_triples);

/* Run N independent decomposition passes with slight variations
 * (different splitting strategies). Returns N decomposition results. */
int tardy_decompose_multi(const char *text, int len,
                           tardy_decomposition_t *decomps, int count);

#endif
