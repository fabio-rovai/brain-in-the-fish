/*
 * Tardygrada — Terraform Protocol
 *
 * Converts agentic GitHub repos into .tardy files.
 * Every discovery about the target repo is a verified claim.
 * Only verified patterns make it into the generated code.
 *
 * Supported frameworks:
 *   - CrewAI (Python: Agent, Task, Crew, @tool)
 *   - AutoGen (Python: ConversableAgent, GroupChat)
 *   - LangChain (Python: chain, AgentExecutor, @tool)
 *   - LangGraph (Python: StateGraph, add_node, add_edge)
 *   - LlamaIndex (Python: QueryEngine, VectorStoreIndex)
 */

#ifndef TARDY_TERRAFORM_PROTO_H
#define TARDY_TERRAFORM_PROTO_H

#include "../vm/types.h"

#define TARDY_TF_MAX_AGENTS    32
#define TARDY_TF_MAX_TOOLS     64
#define TARDY_TF_MAX_TASKS     32
#define TARDY_TF_MAX_NAME      64
#define TARDY_TF_MAX_DESC      256

/* ============================================
 * Discovered patterns from target repo
 * ============================================ */

typedef struct {
    char name[TARDY_TF_MAX_NAME];
    char role[TARDY_TF_MAX_DESC];
    char goal[TARDY_TF_MAX_DESC];
    int  verified;  /* 1 = found in code, 0 = inferred */
} tardy_tf_agent_t;

typedef struct {
    char name[TARDY_TF_MAX_NAME];
    char description[TARDY_TF_MAX_DESC];
    char returns[TARDY_TF_MAX_NAME];  /* return type */
    int  verified;
} tardy_tf_tool_t;

typedef struct {
    char description[TARDY_TF_MAX_DESC];
    char agent[TARDY_TF_MAX_NAME];    /* assigned agent */
    char expected_output[TARDY_TF_MAX_DESC];
    int  verified;
} tardy_tf_task_t;

typedef enum {
    TARDY_TF_UNKNOWN,
    TARDY_TF_CREWAI,
    TARDY_TF_AUTOGEN,
    TARDY_TF_LANGCHAIN,
    TARDY_TF_LANGGRAPH,
    TARDY_TF_LLAMAINDEX,
    TARDY_TF_GENERIC,
} tardy_tf_framework_t;

typedef struct {
    tardy_tf_framework_t framework;
    char                 repo_name[TARDY_TF_MAX_NAME];
    tardy_tf_agent_t     agents[TARDY_TF_MAX_AGENTS];
    int                  agent_count;
    tardy_tf_tool_t      tools[TARDY_TF_MAX_TOOLS];
    int                  tool_count;
    tardy_tf_task_t      tasks[TARDY_TF_MAX_TASKS];
    int                  task_count;
    char                 orchestration[TARDY_TF_MAX_DESC]; /* sequential, hierarchical, etc */
    int                  total_lines;
    int                  total_files;
    int                  total_deps;
} tardy_tf_analysis_t;

/* ============================================
 * API
 * ============================================ */

/* Analyze a directory for agentic patterns */
int tardy_tf_analyze(const char *repo_path, tardy_tf_analysis_t *out);

/* Detect which framework the repo uses */
tardy_tf_framework_t tardy_tf_detect_framework(const char *repo_path);

/* Scan Python files for agent/tool/task patterns */
int tardy_tf_scan_python(const char *path, tardy_tf_analysis_t *out);

/* Generate .tardy file from analysis */
int tardy_tf_generate(const tardy_tf_analysis_t *analysis,
                       char *output, int max_len);

/* Full pipeline: analyze + generate */
int tardy_tf_terraform(const char *repo_path,
                        char *output, int max_len);

#endif /* TARDY_TERRAFORM_PROTO_H */
