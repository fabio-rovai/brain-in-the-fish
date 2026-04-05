/*
 * Tardygrada -- Text Preprocessor
 *
 * Strips markdown, normalizes numbers, extracts key-value pairs
 * from LLM output before decomposition.
 */

#ifndef TARDY_PREPROCESS_H
#define TARDY_PREPROCESS_H

#include "pipeline.h"

/* Strip markdown formatting from text.
 * Removes: **bold**, ##headers, - bullets, * bullets, [links](url)
 * Returns new length. Modifies text in-place. */
int tardy_strip_markdown(char *text, int len);

/* Extract key-value pairs from LLM structured output.
 * Handles: "Key: Value", "**Key:** Value", "- Key: Value"
 * Returns number of triples extracted. */
int tardy_extract_keyvalue(const char *text, int len,
                            const char *subject,
                            tardy_triple_t *triples, int max_triples);

/* Full preprocessing: strip markdown + extract key-values + decompose.
 * This is the subagent-style decomposer that replaces the basic one
 * when it produces too few triples. */
int tardy_preprocess_and_decompose(const char *text, int len,
                                    tardy_triple_t *triples, int max_triples);

#endif
