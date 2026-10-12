export type Todo = { id: string; text: string; done: boolean };

export type GitHubSummary = {
  prNumber: number | null;
  prTitle: string | null;
  prState: string | null;
  prUrl: string | null;
  passed: number;
  failed: number;
  pending: number;
};

export type BranchData = { note: string; todos: Todo[]; github?: GitHubSummary };

/** What the mod shows: this session's repository and branch, and its memo. */
export type View = {
  /** The app's export was found and is recent. */
  appRunning: boolean;
  locale: "ko" | "en";
  repoKey: string | null;
  branch: string;
  data: BranchData | null;
};

declare module "claude-code" {
  interface PluginState {
    "branch-notes": { view: View | null; isHidden: boolean };
  }
}
