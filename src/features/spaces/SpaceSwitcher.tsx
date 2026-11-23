import type { MailApp } from "../shell/useMailApp";
import type { SpaceColor } from "../../types/mail";

const SPACE_CLASSES: Record<SpaceColor, string> = {
  azure: "acct-azure",
  berry: "acct-berry",
  teal: "acct-teal",
  amber: "acct-amber",
  violet: "acct-violet",
  emerald: "acct-emerald",
};

export function spaceClass(color: SpaceColor): string {
  return SPACE_CLASSES[color];
}

export function SpaceSwitcher({ app }: { app: MailApp }) {
  const totalUnread = app.accounts.reduce((sum, a) => sum + a.unread, 0);

  return (
    <nav className="spaces-strip" aria-label="Account spaces">
      <button
        className={`space-chip all-spaces${app.scopeAccount === null ? " active" : ""}`}
        onClick={() => app.selectSpace(null)}
      >
        <span className="acct-dot" />
        All Spaces
        <span className="sp-cnt">{totalUnread}</span>
      </button>

      {app.accounts.map((account) => (
        <button
          key={account.id}
          className={`space-chip ${spaceClass(account.color)}${app.scopeAccount === account.id ? " active" : ""}`}
          onClick={() => app.selectSpace(account.id)}
          title={`${account.displayName} · ${account.email}`}
        >
          <span className="acct-dot" />
          {account.displayName}
          <span className="sp-cnt">{account.unread}</span>
        </button>
      ))}
    </nav>
  );
}
