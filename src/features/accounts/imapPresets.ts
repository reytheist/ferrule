import type { Endpoint } from "../../types/mail";

export type ImapPresetId =
  | "icloud"
  | "yahoo"
  | "fastmail"
  | "zoho"
  | "gmx"
  | "ionos"
  | "custom";

export interface ImapServerPreset {
  readonly id: Exclude<ImapPresetId, "custom">;
  readonly name: string;
  readonly defaultSpaceName: string;
  readonly emailPlaceholder: string;
  readonly domains: readonly string[];
  readonly imap: Endpoint;
  readonly smtp: Endpoint;
  readonly passwordLabel: string;
  readonly passwordPlaceholder: string;
  readonly hint: string;
}

export interface EndpointDraft {
  readonly host: string;
  readonly port: string;
  readonly tls: boolean;
}

export const IMAP_PRESETS: readonly ImapServerPreset[] = [
  {
    id: "icloud",
    name: "iCloud Mail",
    defaultSpaceName: "iCloud",
    emailPlaceholder: "you@icloud.com",
    domains: ["icloud.com", "me.com", "mac.com"],
    imap: { host: "imap.mail.me.com", port: 993, tls: true },
    smtp: { host: "smtp.mail.me.com", port: 587, tls: true },
    passwordLabel: "App-specific password",
    passwordPlaceholder: "xxxx-xxxx-xxxx-xxxx",
    hint: "Generate an App-Specific Password at account.apple.com under Sign-In and Security.",
  },
  {
    id: "yahoo",
    name: "Yahoo Mail",
    defaultSpaceName: "Yahoo",
    emailPlaceholder: "you@yahoo.com",
    domains: ["yahoo.com", "ymail.com", "rocketmail.com"],
    imap: { host: "imap.mail.yahoo.com", port: 993, tls: true },
    smtp: { host: "smtp.mail.yahoo.com", port: 465, tls: true },
    passwordLabel: "App password",
    passwordPlaceholder: "Yahoo app password",
    hint: "Generate an App Password in Yahoo Account Security.",
  },
  {
    id: "fastmail",
    name: "Fastmail",
    defaultSpaceName: "Fastmail",
    emailPlaceholder: "you@fastmail.com",
    domains: [
      "fastmail.com",
      "fastmail.fm",
      "fastmail.org",
      "fastmail.net",
      "fastmail.us",
      "fastmail.uk",
    ],
    imap: { host: "imap.fastmail.com", port: 993, tls: true },
    smtp: { host: "smtp.fastmail.com", port: 465, tls: true },
    passwordLabel: "App password",
    passwordPlaceholder: "Fastmail app password",
    hint: "Create an App Password with Mail access in Fastmail Settings -> Privacy & Security -> Integrations.",
  },
  {
    id: "zoho",
    name: "Zoho Mail",
    defaultSpaceName: "Zoho",
    emailPlaceholder: "you@zohomail.com",
    domains: ["zoho.com", "zohomail.com"],
    imap: { host: "imap.zoho.com", port: 993, tls: true },
    smtp: { host: "smtp.zoho.com", port: 465, tls: true },
    passwordLabel: "Password or app password",
    passwordPlaceholder: "Zoho password or app password",
    hint: "Enable IMAP in Zoho settings. Paid custom-domain orgs use imappro.zoho.com and smtppro.zoho.com; regional DCs use .eu or .in.",
  },
  {
    id: "gmx",
    name: "GMX Mail",
    defaultSpaceName: "GMX",
    emailPlaceholder: "you@gmx.com",
    domains: ["gmx.com", "gmx.us", "gmx.net", "gmx.de"],
    imap: { host: "imap.gmx.com", port: 993, tls: true },
    smtp: { host: "mail.gmx.com", port: 587, tls: true },
    passwordLabel: "Password",
    passwordPlaceholder: "GMX account password",
    hint: "Enable IMAP/POP access in GMX webmail settings. For gmx.de or gmx.net accounts, edit hosts from .com to .net.",
  },
  {
    id: "ionos",
    name: "IONOS",
    defaultSpaceName: "IONOS",
    emailPlaceholder: "you@yourdomain.com",
    domains: ["ionos.com", "ionos.de", "ionos.co.uk", "1and1.com"],
    imap: { host: "imap.ionos.com", port: 993, tls: true },
    smtp: { host: "smtp.ionos.com", port: 587, tls: true },
    passwordLabel: "Password",
    passwordPlaceholder: "IONOS mailbox password",
    hint: "Use your full email address and mailbox password. Regional IONOS accounts may use .de or .co.uk hosts, or SMTP port 465.",
  },
];

export function isImapPresetId(value: string): value is ImapPresetId {
  return value === "custom" || IMAP_PRESETS.some((preset) => preset.id === value);
}

export function findImapPreset(id: ImapPresetId): ImapServerPreset | null {
  if (id === "custom") return null;
  return IMAP_PRESETS.find((preset) => preset.id === id) ?? null;
}

export function detectImapPresetByEmail(email: string): ImapServerPreset | null {
  const parts = email.trim().toLowerCase().split("@");
  if (parts.length !== 2) return null;
  const domain = parts[1] ?? "";
  if (!domain) return null;
  return IMAP_PRESETS.find((preset) => preset.domains.includes(domain)) ?? null;
}

export function formatImapSecurity(tls: boolean): string {
  return tls ? "TLS" : "Plain";
}

// Matches crates/mail-core/src/sync/smtp.rs: port 465 uses implicit TLS; other TLS ports use STARTTLS.
export function formatSmtpSecurity(port: number, tls: boolean): string {
  if (!tls) return "Plain";
  return port === 465 ? "TLS" : "STARTTLS";
}

function matchesEndpoint(expected: Endpoint, actual: EndpointDraft): boolean {
  return (
    actual.host.trim().toLowerCase() === expected.host.toLowerCase() &&
    actual.port.trim() === String(expected.port) &&
    actual.tls === expected.tls
  );
}

export function matchesPresetEndpoints(
  preset: ImapServerPreset,
  imap: EndpointDraft,
  smtp: EndpointDraft,
): boolean {
  return matchesEndpoint(preset.imap, imap) && matchesEndpoint(preset.smtp, smtp);
}

export const PIGMENT_STAFF_IONOS_PRESET: ImapServerPreset = {
  id: "ionos",
  name: "Pigment Staff (IONOS)",
  defaultSpaceName: "Pigment Staff",
  emailPlaceholder: "info@pigmentminiatures.com",
  domains: ["ionos.com", "ionos.de", "ionos.co.uk", "1and1.com"],
  imap: { host: "imap.ionos.com", port: 993, tls: true },
  smtp: { host: "smtp.ionos.com", port: 587, tls: true },
  passwordLabel: "Mailbox password",
  passwordPlaceholder: "IONOS mailbox password",
  hint: "Enter your Pigment role mailbox (info@, sponsors@, exhibition@, marketing@, volunteers@) and IONOS password.",
};

export function suggestPigmentSpaceName(email: string): string {
  const local = email.trim().split("@")[0]?.trim() ?? "";
  if (!local) return "Pigment Staff";
  const titled = local
    .split(/[._-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
    .join(" ");
  return titled ? `Pigment · ${titled}` : "Pigment Staff";
}
