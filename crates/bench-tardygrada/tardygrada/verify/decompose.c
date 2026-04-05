/*
 * Tardygrada — Rule-Based Text Decomposer
 *
 * Breaks free text into (subject, predicate, object) triples
 * using pattern matching on common English sentence structures.
 * No LLM needed. Deterministic. Verifiable.
 */

#include "decompose.h"
#include <string.h>
#include <ctype.h>
#include <stdio.h>

/* Defined in vm/context.c */
extern tardy_uuid_t tardy_uuid_gen(void);

/* ============================================
 * Helpers
 * ============================================ */

/* Trim leading and trailing whitespace in-place into dst.
 * Returns length of trimmed string. */
static int trim_copy(char *dst, int dst_size, const char *src, int src_len)
{
    int start = 0;
    int end = src_len - 1;

    while (start <= end && isspace((unsigned char)src[start]))
        start++;
    while (end >= start && isspace((unsigned char)src[end]))
        end--;

    int len = end - start + 1;
    if (len <= 0) {
        dst[0] = '\0';
        return 0;
    }
    if (len >= dst_size)
        len = dst_size - 1;

    memcpy(dst, src + start, (size_t)len);
    dst[len] = '\0';
    return len;
}

/* Case-insensitive strstr */
static const char *ci_strstr(const char *haystack, const char *needle)
{
    if (!needle[0])
        return haystack;
    int nlen = (int)strlen(needle);
    for (const char *p = haystack; *p; p++) {
        int match = 1;
        for (int i = 0; i < nlen; i++) {
            if (!p[i] || tolower((unsigned char)p[i]) !=
                         tolower((unsigned char)needle[i])) {
                match = 0;
                break;
            }
        }
        if (match)
            return p;
    }
    return NULL;
}

/* Try to extract a triple from a sentence using a keyword pattern.
 * Pattern: subject <keyword> object
 * Returns 1 if matched, 0 otherwise. */
static int try_pattern(const char *sent, int sent_len,
                        const char *keyword, const char *predicate,
                        tardy_triple_t *triple)
{
    const char *pos = ci_strstr(sent, keyword);
    if (!pos)
        return 0;

    int subj_len = (int)(pos - sent);
    int kw_len = (int)strlen(keyword);
    int obj_start = (int)(pos - sent) + kw_len;
    int obj_len = sent_len - obj_start;

    if (subj_len <= 0 || obj_len <= 0)
        return 0;

    trim_copy(triple->subject, TARDY_MAX_TRIPLE_LEN, sent, subj_len);
    trim_copy(triple->object, TARDY_MAX_TRIPLE_LEN,
              sent + obj_start, obj_len);
    strncpy(triple->predicate, predicate, TARDY_MAX_TRIPLE_LEN - 1);
    triple->predicate[TARDY_MAX_TRIPLE_LEN - 1] = '\0';

    /* Skip empty results */
    if (triple->subject[0] == '\0' || triple->object[0] == '\0')
        return 0;

    /* Truncate object at " by " or " at " to get clean object,
     * caller will re-process remainder for additional triples */
    const char *by_pos = ci_strstr(triple->object, " by ");
    const char *at_pos = ci_strstr(triple->object, " at ");
    if (by_pos && (int)(by_pos - triple->object) > 2)
        triple->object[(int)(by_pos - triple->object)] = '\0';
    else if (at_pos && (int)(at_pos - triple->object) > 2)
        triple->object[(int)(at_pos - triple->object)] = '\0';

    return 1;
}

/* ============================================
 * Sentence Splitter
 * ============================================ */

/* Split mode determines which delimiters to use */
typedef enum {
    SPLIT_DOT_ONLY = 0,     /* '.' only */
    SPLIT_DOT_COMMA_AND,    /* '.', ',', 'and' */
    SPLIT_DOT_SEMI_COLON    /* '.', ';', ':' */
} split_mode_t;

/* Check if character is a sentence delimiter for the given mode */
static int is_delim(const char *text, int pos, int len, split_mode_t mode)
{
    char c = text[pos];

    /* Split on '.' but NOT if followed by a letter (handles "C. E. Webber") */
    if (c == '.' || c == '!' || c == '?') {
        if (c == '.' && pos + 2 < len &&
            text[pos + 1] == ' ' &&
            ((text[pos + 2] >= 'A' && text[pos + 2] <= 'Z') ||
             (text[pos + 2] >= 'a' && text[pos + 2] <= 'z')) &&
            /* But DO split if next char starts a new sentence (capital after space-space or after ". It/The/A") */
            !(pos + 2 < len && text[pos + 2] >= 'A' && text[pos + 2] <= 'Z' &&
              (pos + 3 >= len || text[pos + 3] == ' ' || text[pos + 3] >= 'a')))
            return 0; /* abbreviation like "C. E." - don't split */
        return 1;
    }

    if (mode == SPLIT_DOT_COMMA_AND) {
        if (c == ',')
            return 1;
        /* Check for " and " as a delimiter */
        if (pos + 4 < len &&
            text[pos] == ' ' &&
            tolower((unsigned char)text[pos + 1]) == 'a' &&
            tolower((unsigned char)text[pos + 2]) == 'n' &&
            tolower((unsigned char)text[pos + 3]) == 'd' &&
            text[pos + 4] == ' ')
            return 1;
    }

    if (mode == SPLIT_DOT_SEMI_COLON) {
        if (c == ';' || c == ':')
            return 1;
    }

    return 0;
}

/* Get delimiter length (most are 1, " and " is 5) */
static int delim_len(const char *text, int pos, int len, split_mode_t mode)
{
    if (mode == SPLIT_DOT_COMMA_AND && pos + 4 < len &&
        text[pos] == ' ' &&
        tolower((unsigned char)text[pos + 1]) == 'a' &&
        tolower((unsigned char)text[pos + 2]) == 'n' &&
        tolower((unsigned char)text[pos + 3]) == 'd' &&
        text[pos + 4] == ' ')
        return 5;
    return 1;
}

/* ============================================
 * Pattern Table
 * ============================================ */

typedef struct {
    const char *keyword;
    const char *predicate;
} pattern_t;

/* Ordered from most specific to least specific */
static const pattern_t patterns[] = {
    /* Creation / origin */
    { " was created at ",       "created_at"       },
    { " was created by ",       "created_by"       },
    { " was created in ",       "created_in"       },
    { " was founded by ",       "founded_by"       },
    { " was founded in ",       "founded_in"       },
    { " was invented by ",      "invented_by"      },
    { " was discovered by ",    "discovered_by"    },
    { " was built by ",         "built_by"         },
    { " was built in ",         "built_in"         },
    { " was born in ",          "born_in"          },
    { " was born on ",          "born_on"          },
    { " died in ",              "died_in"          },
    { " died on ",              "died_on"          },
    /* Location / containment */
    { " is in ",                "located_in"       },
    { " is located in ",        "located_in"       },
    { " is part of ",           "part_of"          },
    { " belongs to ",           "belongs_to"       },
    { " contains ",             "contains"         },
    /* Properties / attributes */
    { " is known as ",          "known_as"         },
    { " is also known as ",     "also_known_as"    },
    { " is a type of ",         "type_of"          },
    { " is a kind of ",         "kind_of"          },
    { " is an example of ",     "example_of"       },
    { " is a ",                 "is_a"             },
    { " is an ",                "is_an"            },
    /* Relationships */
    { " works for ",            "works_for"        },
    { " works at ",             "works_at"         },
    { " wrote ",                "wrote"            },
    { " directed ",             "directed"         },
    { " produced ",             "produced"         },
    { " starred in ",           "starred_in"       },
    { " invented ",             "invented"         },
    { " discovered ",           "discovered"       },
    { " leads ",                "leads"            },
    { " owns ",                 "owns"             },
    /* Temporal */
    { " started in ",           "started_in"       },
    { " ended in ",             "ended_in"         },
    { " occurred in ",          "occurred_in"      },
    { " happened in ",          "happened_in"      },
    { " premiered on ",         "premiered_on"     },
    { " premiered in ",         "premiered_in"     },
    { " first aired on ",       "first_aired_on"   },
    { " first aired in ",       "first_aired_in"   },
    { " debuted in ",           "debuted_in"       },
    { " released in ",          "released_in"      },
    { " published in ",         "published_in"     },
    { " launched in ",          "launched_in"      },
    /* LLM compound patterns -- split "by X at Y" into separate triples */
    { " was developed by ",     "developed_by"     },
    { " was commissioned by ",  "commissioned_by"  },
    { " was designed by ",      "designed_by"      },
    { " was produced by ",      "produced_by"      },
    { " was written by ",       "written_by"       },
    { " was established in ",   "established_in"   },
    /* Special: "X is the Y of Z" handled separately */
    { " is the ",               NULL               },
    /* Generic fallbacks (least specific) */
    { " is ",                   "is"               },
    { " has ",                  "has"              },
    { " are ",                  "are"              },
    { " was ",                  "was"              },
    { " were ",                 "were"             },
    { " in ",                   "located_in"       },
    { " of ",                   "of"               },
};

#define PATTERN_COUNT ((int)(sizeof(patterns) / sizeof(patterns[0])))

/* Handle "X is the Y of Z" → (Z, has_Y, X) */
static int try_is_the_of(const char *sent, int sent_len,
                           tardy_triple_t *triple)
{
    const char *is_the = ci_strstr(sent, " is the ");
    if (!is_the)
        return 0;

    int subj_len = (int)(is_the - sent);
    const char *after_is_the = is_the + 8; /* len(" is the ") */
    const char *of_pos = ci_strstr(after_is_the, " of ");
    if (!of_pos)
        return 0;

    /* Y is between "is the" and "of" */
    int y_len = (int)(of_pos - after_is_the);
    const char *z_start = of_pos + 4; /* len(" of ") */
    int z_len = sent_len - (int)(z_start - sent);

    if (subj_len <= 0 || y_len <= 0 || z_len <= 0)
        return 0;

    char y_buf[TARDY_MAX_TRIPLE_LEN];
    trim_copy(y_buf, TARDY_MAX_TRIPLE_LEN, after_is_the, y_len);

    /* Build predicate: "has_Y" */
    char pred[TARDY_MAX_TRIPLE_LEN];
    snprintf(pred, TARDY_MAX_TRIPLE_LEN, "has_%.240s", y_buf);

    /* Triple: (Z, has_Y, X) */
    trim_copy(triple->subject, TARDY_MAX_TRIPLE_LEN, z_start, z_len);
    strncpy(triple->predicate, pred, TARDY_MAX_TRIPLE_LEN - 1);
    triple->predicate[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
    trim_copy(triple->object, TARDY_MAX_TRIPLE_LEN, sent, subj_len);

    if (triple->subject[0] == '\0' || triple->object[0] == '\0')
        return 0;

    return 1;
}

/* Handle "The Y of X is Z" → (X, has_Y, Z) */
static int try_the_of_is(const char *sent, int sent_len,
                           tardy_triple_t *triple)
{
    /* Must start with "The " (case-insensitive) */
    if (sent_len < 4)
        return 0;
    if (tolower((unsigned char)sent[0]) != 't' ||
        tolower((unsigned char)sent[1]) != 'h' ||
        tolower((unsigned char)sent[2]) != 'e' ||
        sent[3] != ' ')
        return 0;

    const char *of_pos = ci_strstr(sent + 4, " of ");
    if (!of_pos)
        return 0;

    const char *is_pos = ci_strstr(of_pos + 4, " is ");
    if (!is_pos)
        return 0;

    /* Y: between "The " and " of " */
    int y_len = (int)(of_pos - (sent + 4));
    /* X: between " of " and " is " */
    const char *x_start = of_pos + 4;
    int x_len = (int)(is_pos - x_start);
    /* Z: after " is " */
    const char *z_start = is_pos + 4;
    int z_len = sent_len - (int)(z_start - sent);

    if (y_len <= 0 || x_len <= 0 || z_len <= 0)
        return 0;

    char y_buf[TARDY_MAX_TRIPLE_LEN];
    trim_copy(y_buf, TARDY_MAX_TRIPLE_LEN, sent + 4, y_len);

    char pred[TARDY_MAX_TRIPLE_LEN];
    snprintf(pred, TARDY_MAX_TRIPLE_LEN, "has_%.240s", y_buf);

    trim_copy(triple->subject, TARDY_MAX_TRIPLE_LEN, x_start, x_len);
    strncpy(triple->predicate, pred, TARDY_MAX_TRIPLE_LEN - 1);
    triple->predicate[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
    trim_copy(triple->object, TARDY_MAX_TRIPLE_LEN, z_start, z_len);

    if (triple->subject[0] == '\0' || triple->object[0] == '\0')
        return 0;

    return 1;
}

/* ============================================
 * Core Decomposition
 * ============================================ */

static int decompose_sentence(const char *sent, int sent_len,
                               tardy_triple_t *triples, int max_triples)
{
    if (sent_len <= 0 || max_triples <= 0)
        return 0;

    int count = 0;

    /* Try "The Y of X is Z" first (most specific structural pattern) */
    if (count < max_triples && try_the_of_is(sent, sent_len, &triples[count]))
        count++;

    /* Try "X is the Y of Z" */
    if (count < max_triples && try_is_the_of(sent, sent_len, &triples[count]))
        count++;

    /* If we got a structural match, return it */
    if (count > 0)
        return count;

    /* Try remaining patterns */
    for (int p = 0; p < PATTERN_COUNT && count < max_triples; p++) {
        if (patterns[p].predicate == NULL)
            continue; /* skip special patterns handled above */
        if (try_pattern(sent, sent_len, patterns[p].keyword,
                        patterns[p].predicate, &triples[count]))
            count++;
    }

    return count;
}

/* ============================================
 * Public API
 * ============================================ */

/* Check if a '.' is an abbreviation dot rather than end-of-sentence.
 * Returns 1 if it looks like an abbreviation (e.g. "C. E." or "Dr.") */
static int is_abbreviation_dot(const char *text, int pos, int len)
{
    /* Single capital letter before dot: "C." "E." -- must be preceded by
     * space or start of string to avoid matching "BBC." as abbreviation */
    if (pos >= 1 && (text[pos - 1] >= 'A' && text[pos - 1] <= 'Z') &&
        (pos < 2 || text[pos - 2] == ' ' || text[pos - 2] == '.'))
        return 1;

    /* A dot followed by a space and lowercase letter is likely abbreviation
     * e.g. "Dr. smith" -- but NOT "BBC. It" */
    if (pos + 2 < len && text[pos + 1] == ' ' &&
        (text[pos + 2] >= 'a' && text[pos + 2] <= 'z'))
        return 1;

    /* A dot followed by a space, then a single uppercase letter, then dot:
     * handles mid-name like "C. E." where we're at the first dot */
    if (pos + 4 < len && text[pos + 1] == ' ' &&
        (text[pos + 2] >= 'A' && text[pos + 2] <= 'Z') &&
        text[pos + 3] == '.')
        return 1;

    return 0;
}

int tardy_decompose(const char *text, int len,
                     tardy_triple_t *triples, int max_triples)
{
    if (!text || len <= 0 || !triples || max_triples <= 0)
        return 0;

    int count = 0;
    int sent_start = 0;
    char first_subject[TARDY_MAX_TRIPLE_LEN];
    first_subject[0] = '\0';

    /* Split on sentence boundaries and decompose each */
    for (int i = 0; i <= len; i++) {
        int at_end = (i == len);
        int at_delim = 0;
        if (!at_end && (text[i] == '!' || text[i] == '?'))
            at_delim = 1;
        else if (!at_end && text[i] == '.' && !is_abbreviation_dot(text, i, len))
            at_delim = 1;

        if (at_delim || at_end) {
            int sent_len = i - sent_start;
            if (sent_len > 0 && count < max_triples) {
                /* Trim the sentence */
                char sent[1024];
                int slen = trim_copy(sent, (int)sizeof(sent),
                                     text + sent_start, sent_len);

                /* Pronoun resolution: "It " -> first subject found */
                char resolved[1024];
                if (slen > 3 &&
                    (sent[0] == 'I' || sent[0] == 'i') &&
                    sent[1] == 't' && sent[2] == ' ' &&
                    first_subject[0]) {
                    slen = snprintf(resolved, (int)sizeof(resolved),
                                    "%s%s", first_subject, sent + 2);
                    if (slen >= (int)sizeof(resolved))
                        slen = (int)sizeof(resolved) - 1;
                    memcpy(sent, resolved, (size_t)slen + 1);
                }

                if (slen > 0) {
                    int n = decompose_sentence(sent, slen,
                                                triples + count,
                                                max_triples - count);
                    /* Remember first subject for pronoun resolution */
                    if (n > 0 && !first_subject[0]) {
                        strncpy(first_subject, triples[count].subject,
                                TARDY_MAX_TRIPLE_LEN - 1);
                        first_subject[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
                    }
                    count += n;
                }
            }
            sent_start = i + 1;
        }
    }

    /* Second pass: extract "by X" and "at Y" clauses from full text */
    if (count < max_triples) {
        const char *by = text;
        while ((by = ci_strstr(by, " by ")) != NULL && count < max_triples) {
            by += 4; /* skip " by " */
            char name[TARDY_MAX_TRIPLE_LEN];
            int ni = 0;
            while (by[ni] && by[ni] != ',' && by[ni] != '\n' &&
                   ni < TARDY_MAX_TRIPLE_LEN - 1) {
                /* Stop at '.' only if it looks like end-of-sentence */
                if (by[ni] == '.') {
                    if (ni + 2 < len - (int)(by - text) &&
                        by[ni + 1] == ' ' &&
                        (by[ni + 2] >= 'A' && by[ni + 2] <= 'Z') &&
                        by[ni + 3] == '.')
                        { ni++; continue; } /* abbreviation like "C. E." */
                    break; /* real sentence end */
                }
                /* Stop at " and " or " at " to separate list items */
                if (by[ni] == ' ' && ni + 4 < TARDY_MAX_TRIPLE_LEN &&
                    (ci_strstr(by + ni, " and ") == by + ni ||
                     ci_strstr(by + ni, " at ") == by + ni))
                    break;
                ni++;
            }
            name[ni] = '\0';
            memcpy(name, by, (size_t)ni);
            /* Trim trailing whitespace */
            while (ni > 0 && (name[ni-1] == ' ' || name[ni-1] == '\t'))
                name[--ni] = '\0';

            if (ni > 2) {
                /* Check this triple doesn't duplicate an existing one */
                int dup = 0;
                for (int d = 0; d < count; d++) {
                    if (strcmp(triples[d].predicate, "created_by") == 0 &&
                        strcmp(triples[d].object, name) == 0) {
                        dup = 1; break;
                    }
                }
                if (!dup) {
                    const char *subj = first_subject[0] ?
                        first_subject : (count > 0 ? triples[0].subject : "subject");
                    strncpy(triples[count].subject, subj, TARDY_MAX_TRIPLE_LEN - 1);
                    triples[count].subject[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
                    strncpy(triples[count].predicate, "created_by", TARDY_MAX_TRIPLE_LEN - 1);
                    triples[count].predicate[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
                    strncpy(triples[count].object, name, TARDY_MAX_TRIPLE_LEN - 1);
                    triples[count].object[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
                    count++;
                }
            }
            by += (ni > 0 ? ni : 1);
        }
    }

    if (count < max_triples) {
        /* Look for "at the <Place>" or "at <Place>" patterns */
        const char *at = text;
        while ((at = ci_strstr(at, " at the ")) != NULL && count < max_triples) {
            at += 8; /* skip " at the " */
            char place[TARDY_MAX_TRIPLE_LEN];
            int pi = 0;
            while (at[pi] && at[pi] != ',' && at[pi] != '\n' &&
                   pi < TARDY_MAX_TRIPLE_LEN - 1) {
                if (at[pi] == '.' && !is_abbreviation_dot(at, pi, len - (int)(at - text)))
                    break;
                pi++;
            }
            place[pi] = '\0';
            memcpy(place, at, (size_t)pi);
            while (pi > 0 && (place[pi-1] == ' ' || place[pi-1] == '\t'))
                place[--pi] = '\0';

            if (pi > 2) {
                int dup = 0;
                for (int d = 0; d < count; d++) {
                    if (strcmp(triples[d].predicate, "located_at") == 0 &&
                        strcmp(triples[d].object, place) == 0) {
                        dup = 1; break;
                    }
                }
                if (!dup) {
                    const char *subj = first_subject[0] ?
                        first_subject : (count > 0 ? triples[0].subject : "subject");
                    strncpy(triples[count].subject, subj, TARDY_MAX_TRIPLE_LEN - 1);
                    triples[count].subject[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
                    strncpy(triples[count].predicate, "located_at", TARDY_MAX_TRIPLE_LEN - 1);
                    triples[count].predicate[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
                    strncpy(triples[count].object, place, TARDY_MAX_TRIPLE_LEN - 1);
                    triples[count].object[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
                    count++;
                }
            }
            at += (pi > 0 ? pi : 1);
        }
    }

    /* Also try "at <Place>" without "the" */
    if (count < max_triples) {
        const char *at = text;
        while ((at = ci_strstr(at, " at ")) != NULL && count < max_triples) {
            /* Skip if this is " at the " (already handled) */
            if (strncmp(at + 4, "the ", 4) == 0) { at += 4; continue; }
            at += 4;
            char place[TARDY_MAX_TRIPLE_LEN];
            int pi = 0;
            while (at[pi] && at[pi] != ',' && at[pi] != '\n' &&
                   pi < TARDY_MAX_TRIPLE_LEN - 1) {
                if (at[pi] == '.' && !is_abbreviation_dot(at, pi, len - (int)(at - text)))
                    break;
                pi++;
            }
            place[pi] = '\0';
            memcpy(place, at, (size_t)pi);
            while (pi > 0 && (place[pi-1] == ' ' || place[pi-1] == '\t'))
                place[--pi] = '\0';

            if (pi > 2) {
                int dup = 0;
                for (int d = 0; d < count; d++) {
                    if (strcmp(triples[d].predicate, "located_at") == 0 &&
                        strcmp(triples[d].object, place) == 0) {
                        dup = 1; break;
                    }
                }
                if (!dup) {
                    const char *subj = first_subject[0] ?
                        first_subject : (count > 0 ? triples[0].subject : "subject");
                    strncpy(triples[count].subject, subj, TARDY_MAX_TRIPLE_LEN - 1);
                    triples[count].subject[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
                    strncpy(triples[count].predicate, "located_at", TARDY_MAX_TRIPLE_LEN - 1);
                    triples[count].predicate[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
                    strncpy(triples[count].object, place, TARDY_MAX_TRIPLE_LEN - 1);
                    triples[count].object[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
                    count++;
                }
            }
            at += (pi > 0 ? pi : 1);
        }
    }

    /* Fallback: if no triples extracted, use whole text as one triple */
    if (count == 0 && max_triples > 0) {
        strncpy(triples[0].subject, "claim", TARDY_MAX_TRIPLE_LEN - 1);
        triples[0].subject[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
        strncpy(triples[0].predicate, "states", TARDY_MAX_TRIPLE_LEN - 1);
        triples[0].predicate[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
        int copylen = len < TARDY_MAX_TRIPLE_LEN - 1 ?
                      len : TARDY_MAX_TRIPLE_LEN - 1;
        memcpy(triples[0].object, text, (size_t)copylen);
        triples[0].object[copylen] = '\0';
        count = 1;
    }

    return count;
}

/* Decompose with a specific split mode */
static int decompose_with_mode(const char *text, int len,
                                 tardy_triple_t *triples, int max_triples,
                                 split_mode_t mode)
{
    if (!text || len <= 0 || !triples || max_triples <= 0)
        return 0;

    /* For DOT_ONLY mode, just use the standard decompose */
    if (mode == SPLIT_DOT_ONLY)
        return tardy_decompose(text, len, triples, max_triples);

    int count = 0;
    int sent_start = 0;

    for (int i = 0; i <= len; i++) {
        int at_end = (i == len);
        int at_delim = (!at_end && is_delim(text, i, len, mode));

        if (at_delim || at_end) {
            int dl = at_delim ? delim_len(text, i, len, mode) : 0;
            int sent_len = i - sent_start;
            if (sent_len > 0 && count < max_triples) {
                char sent[1024];
                int slen = trim_copy(sent, (int)sizeof(sent),
                                     text + sent_start, sent_len);
                if (slen > 0) {
                    int n = decompose_sentence(sent, slen,
                                                triples + count,
                                                max_triples - count);
                    count += n;
                }
            }
            sent_start = i + (dl > 0 ? dl : 1);
            if (dl > 1)
                i += dl - 1; /* skip multi-char delimiters */
        }
    }

    /* Fallback */
    if (count == 0 && max_triples > 0) {
        strncpy(triples[0].subject, "claim", TARDY_MAX_TRIPLE_LEN - 1);
        triples[0].subject[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
        strncpy(triples[0].predicate, "states", TARDY_MAX_TRIPLE_LEN - 1);
        triples[0].predicate[TARDY_MAX_TRIPLE_LEN - 1] = '\0';
        int copylen = len < TARDY_MAX_TRIPLE_LEN - 1 ?
                      len : TARDY_MAX_TRIPLE_LEN - 1;
        memcpy(triples[0].object, text, (size_t)copylen);
        triples[0].object[copylen] = '\0';
        count = 1;
    }

    return count;
}

int tardy_decompose_multi(const char *text, int len,
                           tardy_decomposition_t *decomps, int count)
{
    if (!text || len <= 0 || !decomps || count <= 0)
        return 0;

    /* Pass 1: split on '.' only */
    if (count >= 1) {
        memset(&decomps[0], 0, sizeof(tardy_decomposition_t));
        decomps[0].count = decompose_with_mode(
            text, len, decomps[0].triples, TARDY_MAX_TRIPLES,
            SPLIT_DOT_ONLY);
        decomps[0].decomposer = tardy_uuid_gen();
    }

    /* Pass 2: split on '.', ',', 'and' */
    if (count >= 2) {
        memset(&decomps[1], 0, sizeof(tardy_decomposition_t));
        decomps[1].count = decompose_with_mode(
            text, len, decomps[1].triples, TARDY_MAX_TRIPLES,
            SPLIT_DOT_COMMA_AND);
        decomps[1].decomposer = tardy_uuid_gen();
    }

    /* Pass 3: split on '.', ';', ':' */
    if (count >= 3) {
        memset(&decomps[2], 0, sizeof(tardy_decomposition_t));
        decomps[2].count = decompose_with_mode(
            text, len, decomps[2].triples, TARDY_MAX_TRIPLES,
            SPLIT_DOT_SEMI_COLON);
        decomps[2].decomposer = tardy_uuid_gen();
    }

    /* Compute agreement: fraction of triples that appear in all passes */
    int actual = count < 3 ? count : 3;
    for (int i = 0; i < actual; i++) {
        if (actual <= 1) {
            decomps[i].agreement = 1.0f;
            continue;
        }
        int matching = 0;
        for (int t = 0; t < decomps[i].count; t++) {
            int found_in_all = 1;
            for (int j = 0; j < actual; j++) {
                if (j == i) continue;
                int found = 0;
                for (int k = 0; k < decomps[j].count; k++) {
                    if (strcmp(decomps[i].triples[t].subject,
                               decomps[j].triples[k].subject) == 0 &&
                        strcmp(decomps[i].triples[t].predicate,
                               decomps[j].triples[k].predicate) == 0 &&
                        strcmp(decomps[i].triples[t].object,
                               decomps[j].triples[k].object) == 0) {
                        found = 1;
                        break;
                    }
                }
                if (!found) { found_in_all = 0; break; }
            }
            if (found_in_all) matching++;
        }
        decomps[i].agreement = decomps[i].count > 0 ?
            (float)matching / (float)decomps[i].count : 0.0f;
    }

    return actual;
}
