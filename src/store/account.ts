import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { onLocaleApplied } from "../i18n";
import { useGitHubStore } from "./github";

// Mirrors src-tauri/src/sync/account.rs.
export type Provider = "password" | "google.com" | "github.com";

export interface Profile {
  email: string | null;
  emailVerified: boolean;
  name: string | null;
  providers: Provider[];
}

export interface AccountInfo {
  /** This build has a sync server. */
  configured: boolean;
  uid: string | null;
  profile: Profile | null;
}

/** What is in progress (one at a time). */
export type Busy = "email" | "google" | "github" | "reset" | "verify" | "unlink" | "signOut";

interface AccountStore {
  info: AccountInfo | null;
  busy: Busy | null;
  error: string | null;
  /** A success to show (reset mail sent, method linked). */
  notice: string | null;
  dialog: boolean;
  /** GitHub sign-in waits for the work tab's device flow, then continues. */
  pendingGitHub: "signIn" | "link" | null;

  init: () => Promise<void>;
  openDialog: () => void;
  closeDialog: () => void;
  refresh: () => Promise<void>;
  signIn: (email: string, password: string) => Promise<boolean>;
  signUp: (email: string, password: string) => Promise<boolean>;
  resetPassword: (email: string, sent: string) => Promise<void>;
  sendVerification: (sent: string) => Promise<void>;
  addPassword: (email: string, password: string) => Promise<boolean>;
  google: (link: boolean) => Promise<void>;
  github: (link: boolean) => Promise<void>;
  cancel: () => void;
  unlink: (provider: Provider) => Promise<void>;
  signOut: () => Promise<void>;
}

export const useAccountStore = create<AccountStore>((set, get) => {
  let listening = false;
  /** The Google sign-in was cancelled here: its "cancelled" error is no news. */
  let cancelled = false;

  /** Runs one account command; returns whether it worked. */
  const run = async (busy: Busy, f: () => Promise<unknown>) => {
    if (!isTauri() || get().busy) return false;
    set({ busy, error: null, notice: null });
    try {
      const result = await f();
      // Commands that change the account answer with it.
      if (result && typeof result === "object" && "configured" in result)
        set({ info: result as AccountInfo });
      return true;
    } catch (e) {
      set({ error: String(e) });
      return false;
    } finally {
      set({ busy: null });
    }
  };

  const continueGitHub = (link: boolean) =>
    run("github", () => invoke<AccountInfo>("account_github", { link }));

  return {
    info: null,
    busy: null,
    error: null,
    notice: null,
    dialog: false,
    pendingGitHub: null,

    init: async () => {
      if (!isTauri()) return;
      if (!listening) {
        listening = true;
        void listen<AccountInfo>("account", (e) => set({ info: e.payload }));
        // The GitHub device flow finished (or ended): go on with the sign-in.
        useGitHubStore.subscribe((s, prev) => {
          const pending = get().pendingGitHub;
          if (!pending) return;
          if (s.status?.account && !prev.status?.account) {
            set({ pendingGitHub: null });
            void continueGitHub(pending === "link");
          } else if (!s.login && prev.login) {
            // Cancelled, denied or expired.
            set({ pendingGitHub: null, error: s.loginError });
          }
        });
      }
      try {
        set({ info: await invoke<AccountInfo>("account_status", { refresh: false }) });
      } catch (e) {
        set({ error: String(e) });
      }
    },

    openDialog: () => {
      set({ dialog: true, error: null, notice: null });
      void get().refresh();
    },
    closeDialog: () => {
      get().cancel();
      set({ dialog: false, error: null, notice: null });
    },

    refresh: async () => {
      if (!isTauri() || !get().info?.uid) return;
      try {
        set({ info: await invoke<AccountInfo>("account_status", { refresh: true }) });
      } catch {
        // Offline: the saved profile stays.
      }
    },

    signIn: (email, password) =>
      run("email", () => invoke<AccountInfo>("account_sign_in", { email, password })),
    signUp: (email, password) =>
      run("email", () => invoke<AccountInfo>("account_sign_up", { email, password })),
    resetPassword: async (email, sent) => {
      if (await run("reset", () => invoke("account_reset_password", { email })))
        set({ notice: sent });
    },
    sendVerification: async (sent) => {
      if (await run("verify", () => invoke("account_send_verification"))) set({ notice: sent });
    },
    addPassword: (email, password) =>
      run("email", () => invoke<AccountInfo>("account_add_password", { email, password })),

    google: async (link) => {
      cancelled = false;
      const ok = await run("google", () => invoke<AccountInfo>("account_google", { link }));
      if (!ok && cancelled) set({ error: null });
    },

    github: async (link) => {
      if (!isTauri() || get().busy) return;
      const gh = useGitHubStore.getState();
      if (!gh.status) await gh.init();
      if (useGitHubStore.getState().status?.account) {
        await continueGitHub(link);
        return;
      }
      set({ pendingGitHub: link ? "link" : "signIn", error: null, notice: null });
      await useGitHubStore.getState().startLogin();
      const after = useGitHubStore.getState();
      if (!after.login && after.loginError) set({ pendingGitHub: null, error: after.loginError });
    },

    cancel: () => {
      if (get().busy === "google") {
        cancelled = true;
        void invoke("account_google_cancel").catch(() => {});
      }
      if (get().pendingGitHub) {
        set({ pendingGitHub: null });
        useGitHubStore.getState().cancelLogin();
      }
    },

    unlink: async (provider) => {
      await run("unlink", () => invoke<AccountInfo>("account_unlink", { provider }));
    },

    signOut: async () => {
      await run("signOut", () => invoke<AccountInfo>("account_sign_out"));
    },
  };
});

// Errors come from the backend in the old language: drop them.
onLocaleApplied(() => useAccountStore.setState({ error: null }));
