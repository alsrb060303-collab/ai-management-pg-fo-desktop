import { CircleCheck, CircleUserRound, Clock, ExternalLink, Mail, X } from "lucide-react";
import { type FormEvent, type ReactNode, useEffect, useState } from "react";
import { Button, IconButton } from "../../components/ui";
import { useT } from "../../i18n";
import { type Provider, useAccountStore } from "../../store/account";
import { useGitHubStore } from "../../store/github";
import { open } from "../work/GitHubSection";
import { GitHubMark, GoogleMark } from "./BrandIcons";

const PROVIDERS: Provider[] = ["password", "google.com", "github.com"];
const METHOD = {
  password: "account.method.password",
  "google.com": "account.method.google",
  "github.com": "account.method.github",
} as const;

const input =
  "w-full min-w-0 rounded-lg border border-border bg-surface px-2.5 py-1.5 text-sm outline-none focus:border-accent";

/** Sign in, sign up, or manage the app account (in a dialog over the window). */
export function AccountDialog() {
  const t = useT();
  const { dialog, info, closeDialog } = useAccountStore();

  useEffect(() => {
    if (!dialog) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && closeDialog();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dialog, closeDialog]);

  if (!dialog) return null;
  const signedIn = !!info?.uid;
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-6"
      onPointerDown={(e) => e.target === e.currentTarget && closeDialog()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("account.title")}
        className="flex max-h-full w-full max-w-md flex-col rounded-2xl border border-border bg-surface-raised shadow-card"
      >
        <header className="flex items-center gap-2 px-5 pt-4 pb-1">
          <h2 className="font-serif text-lg font-medium">
            {signedIn ? t("account.title") : t("account.signIn")}
          </h2>
          <IconButton label={t("account.close")} onClick={closeDialog} className="ml-auto">
            <X size={17} />
          </IconButton>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 pt-1 pb-5">
          {info && !info.configured ? (
            <p className="text-sm text-text-muted">{t("account.notConfigured")}</p>
          ) : signedIn ? (
            <Manage />
          ) : (
            <SignInForm />
          )}
        </div>
      </div>
    </div>
  );
}

function Messages() {
  const { error, notice } = useAccountStore();
  return (
    <>
      {error && <p className="selectable text-xs text-danger">{error}</p>}
      {notice && <p className="text-xs text-success">{notice}</p>}
    </>
  );
}

function SignInForm() {
  const t = useT();
  const { busy, signIn, signUp, resetPassword } = useAccountStore();
  const [mode, setMode] = useState<"signIn" | "signUp">("signIn");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!email.trim() || !password) return;
    void (mode === "signIn" ? signIn : signUp)(email, password);
  };

  return (
    <div className="space-y-4">
      <p className="text-sm text-text-muted">{t("account.intro")}</p>
      <div className="flex gap-1 rounded-lg bg-sunken p-0.5 text-sm">
        {(["signIn", "signUp"] as const).map((m) => (
          <button
            key={m}
            type="button"
            onClick={() => setMode(m)}
            aria-pressed={mode === m}
            className={`flex-1 rounded-md py-1 ${
              mode === m ? "bg-surface-raised font-medium shadow-sm" : "text-text-muted"
            }`}
          >
            {t(m === "signIn" ? "account.signIn" : "account.signUp")}
          </button>
        ))}
      </div>
      <form onSubmit={submit} className="space-y-2.5">
        <label className="block">
          <span className="mb-1 block text-xs font-medium text-text-muted">
            {t("account.email")}
          </span>
          <input
            type="email"
            autoComplete="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className={input}
          />
        </label>
        <label className="block">
          <span className="mb-1 block text-xs font-medium text-text-muted">
            {t("account.password")}
            {mode === "signUp" && (
              <span className="ml-1.5 font-normal text-text-subtle">
                {t("account.passwordHint")}
              </span>
            )}
          </span>
          <input
            type="password"
            autoComplete={mode === "signIn" ? "current-password" : "new-password"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className={input}
          />
        </label>
        <Messages />
        <Button
          type="submit"
          variant="primary"
          className="w-full"
          disabled={!!busy || !email.trim() || !password}
        >
          <Mail size={15} />
          {busy === "email"
            ? t("account.working")
            : t(mode === "signIn" ? "account.submit.signIn" : "account.submit.signUp")}
        </Button>
        {mode === "signIn" && (
          <button
            type="button"
            disabled={!email.trim() || !!busy}
            onClick={() => void resetPassword(email, t("account.resetSent"))}
            className="text-xs text-text-subtle hover:text-accent hover:underline disabled:opacity-50"
          >
            {t("account.forgot")}
          </button>
        )}
      </form>
      <div className="flex items-center gap-3 text-xs text-text-subtle">
        <span className="h-px flex-1 bg-border" />
        {t("account.or")}
        <span className="h-px flex-1 bg-border" />
      </div>
      <ProviderButtons link={false} />
    </div>
  );
}

/** Google and GitHub, with what each is waiting for. */
function ProviderButtons({ link }: { link: boolean }) {
  const t = useT();
  const { busy, pendingGitHub, google, github, cancel } = useAccountStore();
  if (busy === "google") return <Waiting text={t("account.googleWaiting")} onCancel={cancel} />;
  if (pendingGitHub) return <GitHubCode onCancel={cancel} />;
  if (link) return null;
  return (
    <div className="space-y-2">
      <Button className="w-full" disabled={!!busy} onClick={() => void google(false)}>
        <GoogleMark />
        {t("account.google")}
      </Button>
      <Button className="w-full" disabled={!!busy} onClick={() => void github(false)}>
        <GitHubMark />
        {busy === "github" ? t("account.working") : t("account.github")}
      </Button>
      <p className="text-[11px] text-text-subtle">{t("account.githubHint")}</p>
    </div>
  );
}

function Waiting({ text, onCancel }: { text: ReactNode; onCancel: () => void }) {
  const t = useT();
  return (
    <div className="flex items-center gap-2 rounded-lg border border-border bg-surface px-3 py-2.5 text-xs text-text-muted">
      <Clock size={13} className="shrink-0 animate-pulse" />
      <span className="flex-1">{text}</span>
      <button
        type="button"
        onClick={onCancel}
        className="rounded px-1.5 py-0.5 hover:bg-hover hover:text-text"
      >
        {t("account.cancel")}
      </button>
    </div>
  );
}

function GitHubCode({ onCancel }: { onCancel: () => void }) {
  const t = useT();
  const login = useGitHubStore((s) => s.login);
  if (!login) return <Waiting text={t("account.working")} onCancel={onCancel} />;
  return (
    <div className="space-y-2.5 rounded-lg border border-border bg-surface p-3">
      <p className="text-xs text-text-muted">{t("account.githubWaiting")}</p>
      <p className="selectable text-center font-mono text-2xl font-semibold tracking-[0.2em]">
        {login.userCode}
      </p>
      <Button
        variant="primary"
        size="sm"
        className="w-full"
        onClick={() => {
          void navigator.clipboard.writeText(login.userCode).catch(() => {});
          open(login.verificationUri);
        }}
      >
        {t("account.copyAndOpen")}
        <ExternalLink size={12} />
      </Button>
      <Waiting text={login.verificationUri} onCancel={onCancel} />
    </div>
  );
}

function Manage() {
  const t = useT();
  const { info, busy, sendVerification, signOut } = useAccountStore();
  const profile = info?.profile;
  const [adding, setAdding] = useState(false);
  if (!info?.uid) return null;
  const providers = profile?.providers ?? [];

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <CircleUserRound size={36} strokeWidth={1.5} className="shrink-0 text-text-subtle" />
        <div className="min-w-0">
          <p className="truncate font-medium">{profile?.name ?? profile?.email ?? info.uid}</p>
          {profile?.email && (
            <p className="flex items-center gap-1 truncate text-xs text-text-muted">
              {profile.email}
              {profile.emailVerified && (
                <CircleCheck
                  size={12}
                  className="shrink-0 text-success"
                  aria-label={t("account.verified")}
                />
              )}
            </p>
          )}
        </div>
      </div>

      {profile?.email && !profile.emailVerified && providers.includes("password") && (
        <div className="flex items-center gap-2 rounded-lg bg-warning/10 px-3 py-2 text-xs">
          <span className="flex-1 text-warning">{t("account.unverified")}</span>
          <Button
            size="sm"
            variant="ghost"
            disabled={!!busy}
            onClick={() => void sendVerification(t("account.verificationSent"))}
          >
            {t("account.sendVerification")}
          </Button>
        </div>
      )}

      <section>
        <h3 className="text-xs font-semibold text-text-muted">{t("account.methods")}</h3>
        <p className="mt-0.5 mb-2 text-[11px] text-text-subtle">{t("account.methodsHint")}</p>
        <ul className="divide-y divide-border rounded-lg border border-border">
          {PROVIDERS.map((p) => (
            <MethodRow
              key={p}
              provider={p}
              linked={providers.includes(p)}
              canUnlink={providers.length > 1}
              onAddPassword={() => setAdding(true)}
            />
          ))}
        </ul>
        {adding && !providers.includes("password") && (
          <AddPassword email={profile?.email ?? ""} onDone={() => setAdding(false)} />
        )}
        <div className="mt-2">
          <ProviderButtons link />
        </div>
      </section>

      <Messages />

      <div className="flex items-center gap-3 border-t border-border pt-3">
        <div className="min-w-0 flex-1">
          <p className="text-[11px] text-text-subtle">{t("account.signOutHint")}</p>
          <p className="selectable truncate font-mono text-[10.5px] text-text-subtle">
            {t("account.id")}: {info.uid}
          </p>
        </div>
        <Button size="sm" disabled={!!busy} onClick={() => void signOut()}>
          {t("account.signOut")}
        </Button>
      </div>
    </div>
  );
}

function MethodRow({
  provider,
  linked,
  canUnlink,
  onAddPassword,
}: {
  provider: Provider;
  linked: boolean;
  canUnlink: boolean;
  onAddPassword: () => void;
}) {
  const t = useT();
  const { busy, google, github, unlink } = useAccountStore();
  const link = () => {
    if (provider === "password") onAddPassword();
    else if (provider === "google.com") void google(true);
    else void github(true);
  };
  return (
    <li className="flex items-center gap-2 px-3 py-2 text-sm">
      <span className="flex flex-1 items-center gap-2">
        {provider === "google.com" ? (
          <GoogleMark size={14} />
        ) : provider === "github.com" ? (
          <GitHubMark size={14} />
        ) : (
          <Mail size={14} className="shrink-0" />
        )}
        {t(METHOD[provider])}
      </span>
      {linked ? (
        <>
          <span className="flex items-center gap-1 text-xs text-success">
            <CircleCheck size={12} />
            {t("account.linked")}
          </span>
          {canUnlink && (
            <Button
              size="sm"
              variant="ghost"
              disabled={!!busy}
              onClick={() => void unlink(provider)}
            >
              {t("account.unlink")}
            </Button>
          )}
        </>
      ) : (
        <Button size="sm" disabled={!!busy} onClick={link}>
          {provider === "password" ? t("account.addPassword") : t("account.link")}
        </Button>
      )}
    </li>
  );
}

function AddPassword({ email: initial, onDone }: { email: string; onDone: () => void }) {
  const t = useT();
  const { busy, addPassword } = useAccountStore();
  const [email, setEmail] = useState(initial);
  const [password, setPassword] = useState("");
  return (
    <form
      className="mt-2 space-y-2 rounded-lg border border-border bg-surface p-3"
      onSubmit={(e) => {
        e.preventDefault();
        void addPassword(email, password).then((ok) => ok && onDone());
      }}
    >
      <input
        type="email"
        autoComplete="email"
        placeholder={t("account.email")}
        aria-label={t("account.email")}
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        className={input}
      />
      <input
        type="password"
        autoComplete="new-password"
        placeholder={`${t("account.password")} (${t("account.passwordHint")})`}
        aria-label={t("account.password")}
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        className={input}
      />
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="ghost" onClick={onDone}>
          {t("account.cancel")}
        </Button>
        <Button
          size="sm"
          variant="primary"
          type="submit"
          disabled={!!busy || !email.trim() || !password}
        >
          {busy === "email" ? t("account.working") : t("account.addPassword")}
        </Button>
      </div>
    </form>
  );
}
