// loregrep-pi — a pi coding-agent extension exposing loregrep's structural
// code-search tools. Each tool shells out to `loregrep exec-tool` and returns
// its JSON result to the agent.
//
// Install (from the pi package registry):  pi install npm:loregrep-pi
// Requires the `loregrep` binary on PATH (`cargo install loregrep` or `pip install loregrep`).

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

// A `path` (directory to analyze) is common to every tool; loregrep takes it as
// a CLI flag, while the remaining fields are passed as the --params JSON.
const PATH = Type.Optional(
  Type.String({ description: "Directory to analyze (defaults to '.')" }),
);
const LIMIT = Type.Optional(
  Type.Number({ description: "Maximum number of results" }),
);
const LANGUAGE = Type.Optional(
  Type.String({ description: "Filter by language: rust | python | typescript" }),
);

export default function (pi: ExtensionAPI) {
  // Register one loregrep tool. `schema` describes the loregrep params (plus the
  // shared `path`); execute() splits `path` off to --path and sends the rest as --params.
  const tool = (name: string, description: string, schema: Record<string, unknown>) =>
    pi.registerTool({
      name,
      label: name,
      description,
      parameters: Type.Object({ ...schema, path: PATH }),
      async execute(_toolCallId, params: Record<string, unknown>, signal) {
        const { path = ".", ...toolParams } = params;
        const result = await pi.exec(
          "loregrep",
          ["exec-tool", name, "--params", JSON.stringify(toolParams), "--path", String(path)],
          { signal },
        );
        // stdout is pure JSON; on failure loregrep exits non-zero and explains on stderr.
        if (result.exitCode !== 0) {
          return {
            content: [{ type: "text", text: result.stderr || result.stdout || `loregrep exited ${result.exitCode}` }],
            isError: true,
          };
        }
        return { content: [{ type: "text", text: result.stdout }] };
      },
    });

  tool(
    "search_functions",
    "Find functions by name or regex pattern (Rust/Python/TypeScript). Returns names, signatures, file paths, and line numbers.",
    { pattern: Type.String({ description: "Pattern or regex to match function names" }), limit: LIMIT, language: LANGUAGE },
  );

  tool(
    "search_structs",
    "Find structs/classes/interfaces by name or regex pattern. Returns names, fields, file paths, and line numbers.",
    { pattern: Type.String({ description: "Pattern or regex to match struct/class names" }), limit: LIMIT, language: LANGUAGE },
  );

  tool(
    "find_callers",
    "Find all call sites of a function across the repository.",
    { function_name: Type.String({ description: "Function to find callers for" }), limit: LIMIT },
  );

  tool(
    "get_dependencies",
    "Get a file's imports and exports.",
    { file_path: Type.String({ description: "File to analyze dependencies for" }) },
  );

  tool(
    "analyze_file",
    "Get a file's structured skeleton: functions, structs, imports, exports, and calls.",
    {
      file_path: Type.String({ description: "File to analyze" }),
      include_content: Type.Optional(Type.Boolean({ description: "Include raw source in the response" })),
    },
  );

  tool(
    "get_repository_tree",
    "Get a repository overview / directory tree, optionally with per-file skeletons.",
    {
      include_file_details: Type.Optional(Type.Boolean({ description: "Include per-file functions/structs (false for a lightweight overview)" })),
      max_depth: Type.Optional(Type.Number({ description: "Max directory depth (0 = unlimited, 1 = shallow overview)" })),
    },
  );
}
