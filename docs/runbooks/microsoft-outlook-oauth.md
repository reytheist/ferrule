# Microsoft Entra OAuth Setup for Outlook IMAP/SMTP

## Completion test

Click **Add account -> Outlook · Sign in with Microsoft** in Ferrule, paste the Microsoft Entra Application (client) ID once, complete browser sign-in against your live Outlook account, and confirm real inbox threads load with `SyncState::Idle`.

## Prerequisites

- A free Microsoft account (personal Outlook/Hotmail/Live or Microsoft 365 work/school).
- Access to the Microsoft Entra / Azure Portal App Registrations page (`https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade`).

## Proven steps

1. Open [Azure App Registrations](https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade) and sign in with your Microsoft account. Confirm **+ New registration** is visible.
2. Click **+ New registration**, set **Name** to `Ferrule`, choose **Accounts in any organizational directory (Any Microsoft Entra ID tenant - Multitenant) and personal Microsoft accounts**, set **Redirect URI** platform to **Public client/native (mobile & desktop)** with URI `http://localhost`, and click **Register**. Confirm the **Overview** page shows the **Application (client) ID**.
3. Under **Manage -> API permissions**, click **+ Add a permission -> Microsoft Graph -> Delegated permissions**, check `IMAP.AccessAsUser.All`, `SMTP.Send`, `offline_access`, `openid`, `email`, and `profile`, and click **Add permissions**. (No admin consent is needed for a personal Outlook account.)
4. In Ferrule, click **Add account -> Outlook · Sign in with Microsoft**, paste the **Application (client) ID**, leave **Tenant** set to `common`, click **Continue to Microsoft**, select your Outlook account in the browser, and approve access.

## Verification

- **Result**: Passed on live personal Outlook account (2026-11-24). Browser PKCE sign-in succeeded, email was extracted from the OIDC `id_token`, a unique space color was assigned automatically, and real IMAP inbox threads synced cleanly.
- **Implementation notes verified live**:
  - Requesting `openid email profile offline_access https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send` avoids Microsoft Entra v2 mixed-resource scope errors and returns an `id_token` carrying the account email without calling Microsoft Graph.
  - Using `http://localhost:<ephemeral-port>` (no `/callback` path) matches the `http://localhost` Public client redirect URI registered in Azure Portal.

## Env var and credential inventory

| Name | Source | Stored in | Visibility |
| --- | --- | --- | --- |
| Application (client) ID | Azure Portal -> App registrations -> Overview | `localStorage` (`ferrule.outlookClientId`) and SQLite `account.auth_json` | Public |
| Tenant | `common` (supports personal Microsoft accounts and organizational accounts) | `localStorage` (`ferrule.outlookTenant`) and SQLite `account.auth_json` | Public |
| OAuth Refresh Token | Returned after browser PKCE sign-in | OS Keychain (`ferrule:v1:<account_id>:oauth-refresh`) | Secret |
| OAuth Access Token | Refreshed on demand via PKCE | In-memory cache only | Secret |
