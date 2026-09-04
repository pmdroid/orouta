# Auth

Clients send a key from `auth.keys`. Bearer and Anthropic `x-api-key` both work. Missing or wrong keys are 401.

## Sub-features

- `auth-bearer` `Authorization: Bearer` matching a configured key is allowed.
- `auth-x-api-key` `x-api-key` matching a configured key is allowed.
- `auth-missing` no auth header is 401 when keys are non-empty.
- `auth-wrong` a non-matching token is 401.

## How to get to it (user POV)

- Ollama/OpenAI clients: `Authorization: Bearer <key>`.
- Anthropic clients: `x-api-key: <key>`.
- Any path on the instance (auth is global).

## Driving it with curl

Preconditions:

- Instance from `up.sh` (keys non-empty).
- `doctor.sh` passed.

- **Bearer.** `curl -sS -o /tmp/auth-ok.json -w "%{http_code}" -H "Authorization: Bearer $KEY" "$URL/api/tags"`. Status `200`.
- **x-api-key.** Same URL with `-H "x-api-key: $KEY"`. Status `200`.
- **Missing.** `curl -sS -o /dev/null -w "%{http_code}" "$URL/api/tags"`. Status `401`.
- **Wrong.** `curl -sS -o /dev/null -w "%{http_code}" -H "Authorization: Bearer sk-wrong" "$URL/api/tags"`. Status `401`.
- **Proof.** Save the four status codes in `evidence/<run>/auth.txt` next to the 200 body.

## Gotchas

- `anthropic-version` is ignored; it is not auth.
- Empty `auth.keys` is open. Do not use that config for this feature.
- Trailing space in the Bearer token is a miss.
