# Google Cloud — Desktop OAuth Client ID for Ferrule

## Completion test
Clicking **Sign in with Google** inside Ferrule opens Google's sign-in screen in the browser, completes the localhost callback with no error, and syncs real inbox threads for `reytheist@gmail.com` using Google Cloud OAuth in **Testing** mode on your personal Google account.

## Proven steps
1. Open `console.cloud.google.com`, create a project named `Ferrule`, and select it in the top-left project dropdown.
2. Open **Google Auth Platform**, click **Get started**, set App name to `Ferrule`, User support email to your Gmail address, Audience to **External**, Contact information to your Gmail address, check the policy box, and click **Create**.
3. Open **Google Auth Platform → Audience**, leave **Publishing status** in **Testing**, click **+ Add users** under **Test users**, add every Gmail address you plan to connect in Ferrule, and click **Save**.
4. Open **Google Auth Platform → Clients**, click **+ Create client**, choose **Desktop app**, leave **Use this client for an AI-powered agent** unchecked, name it `Ferrule Desktop`, and click **Create**.
5. Copy the **Client ID** and **Client secret** directly from the popup (or downloaded JSON) into Ferrule under **Add Account → Gmail · Sign in with Google**.
6. Click **Sign in with Google**, click **Continue** on Google's "Google hasn't verified this app" Testing-mode screen, approve Gmail access, and confirm the browser shows **Signed in to Ferrule**.
7. To add more Gmail accounts later, make sure each address is listed under **Audience → Test users**, then open **Add Account → Gmail · Sign in with Google** (the Client ID is pre-filled from local config).

## Verification
- Passed on 2026-11-23. Live browser PKCE sign-in and `XOAUTH2` IMAP sync succeeded for `reytheist` (`SyncState::Idle`, 21 inbox threads / 21 messages stored locally in SQLite, refresh token and client secret stored only in Windows Credential Manager).
- Note on Testing mode: Google expires Testing-mode refresh tokens after 7 days; clicking **Sign in again** on the account banner re-authenticates in one click without removing the account.

## Credential inventory

| Name | Where it comes from | Where it is stored | Public or secret |
|---|---|---|---|
| Google OAuth Client ID | Google Cloud Console → Google Auth Platform → Clients → Desktop app | Ferrule local config (`localStorage` + `account.auth_json`) | Public |
| Google OAuth Client Secret | Google Cloud Console → Google Auth Platform → Clients → Desktop app | OS Keychain (Windows Credential Manager / Linux Secret Service) | Secret |
| OAuth Refresh Token | Issued during browser PKCE sign-in | OS Keychain (Windows Credential Manager / Linux Secret Service) | Secret |
| OAuth Access Token | Exchanged from Refresh Token on demand | Memory only (`MailCore` token cache, never persisted) | Secret (ephemeral) |
