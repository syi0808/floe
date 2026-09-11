# OAuth deployment configuration

## Ownership

The server that executes a connector owns its OAuth flow and Person-scoped tokens. Floe clients
start authorization, open the returned system-browser URL, and poll status without receiving the
provider credential. Floe Cloud and self-hosted servers use the same runtime boundary but different
provider registrations.

OAuth client IDs are public identifiers. Client secrets, refresh tokens, and token-encryption keys
are secrets and must not be committed or included in a client build.

## Configuration loading

`floe-server` reads `.env` from its working directory during startup. Existing process environment
variables take precedence. `FLOE_ENV_FILE=/absolute/path/to/file` selects an explicit file; failure
to read an explicitly selected file stops startup.

Use `server/.env` only for local development. It is ignored by Git. Start a new configuration from
`server/.env.example`. Production deployments should inject process environment variables or mount
a secret file with mode `0600` instead of baking credentials into an image.

## Current local profile

The current executable implements the local migration profile only:

| Provider | Registration | Required configuration | Secret |
| --- | --- | --- | --- |
| Google | Desktop application, loopback PKCE | `FLOE_GOOGLE_OAUTH_CLIENT_ID` | Optional for this profile |
| Slack | PKCE public client, exact localhost redirect | `FLOE_SLACK_OAUTH_CLIENT_ID` | Not required |
| GitHub | GitHub App Device Flow | `FLOE_GITHUB_OAUTH_CLIENT_ID` | Not required |
| Microsoft | Desktop public client | `FLOE_MICROSOFT_OAUTH_CLIENT_ID` | Optional; integration deferred |

The Slack registration must contain
`http://localhost:1456/oauth/slack/callback`. Google uses a random loopback port. GitHub Device Flow
does not use a redirect URI. Missing provider configuration makes only that connector unavailable.

## Floe Cloud target

Floe Cloud uses Floe-managed provider registrations, fixed HTTPS callback URLs, and credentials
in the Cloud secret manager. Google requires a separate Web application registration; the Desktop
client ID used by the local profile cannot be reused. Conventional hosted Google and Slack flows
require their client secrets. The current GitHub Device Flow requires only its GitHub App client ID.

Expected variables are:

```dotenv
FLOE_EXTERNAL_URL=https://api.floe.example
FLOE_GOOGLE_OAUTH_CLIENT_ID=
FLOE_GOOGLE_OAUTH_CLIENT_SECRET=
FLOE_SLACK_OAUTH_CLIENT_ID=
FLOE_SLACK_OAUTH_CLIENT_SECRET=
FLOE_GITHUB_OAUTH_CLIENT_ID=
```

Callback URLs must be derived from the trusted `FLOE_EXTERNAL_URL`, never from an incoming `Host`
header. External HTTPS callbacks and hosted encrypted token storage are planned and are not yet
implemented by the current loopback-only server.

## Self-hosted target

Each self-host operator registers provider applications for their own external URL and supplies the
same variables through their deployment environment. Floe's shared Cloud registrations do not
accept arbitrary self-host callback URLs, and Floe does not relay authorization codes to private
servers. A provider remains unavailable until its required client ID, client secret, callback URL,
and scopes are configured.
