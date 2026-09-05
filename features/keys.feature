Feature: manage api keys from the UI

  Scenario: create a key and see it only once
    Given auth.keys contains sk-orouta-alice
    When POST /api/keys with label ci
    Then the response status is 200
    And the secret matches orouta_<32 hex> and appears exactly once in the response
    And the response keys list has the TOML key and the key labeled ci
    And the overlay file records the added key with label, secret and created
    And the full secret never appears in the keys list entries

  Scenario: key ids are stable per key
    Given a key was created
    When another key is created and the first added key is revoked by id
    Then the remaining keys keep the same ids they were listed with
    And a stale id can never resolve to a different key

  Scenario: a created key authorizes requests immediately
    Given a key was created
    When a request uses the new secret
    Then the response status is 200

  Scenario: keys page lists label, prefix, created and last_used without the secret
    Given a key was created and used
    When GET /keys
    Then the body lists the TOML key as "from orouta.toml" with created "in config file"
    And the body shows the created key label and a 12-char prefix but never the full secret
    And the used key shows "just now" as last used

  Scenario: revoke stops the key on the next request
    Given a key was created
    When DELETE /api/keys/{id of the created key}
    Then the response status is 200
    And the overlay file records the secret as revoked
    When a request uses the revoked secret
    Then the response status is 401
    And the previous key still works

  Scenario: revoking a TOML key survives a config edit
    Given the TOML key sk-orouta-alice was revoked
    When orouta.toml is edited to keep sk-orouta-alice and add sk-orouta-bob
    And the config reloads
    Then requests with sk-orouta-bob are accepted
    And requests with sk-orouta-alice are rejected

  Scenario: unknown key id returns 404
    When DELETE /api/keys/kdeadbeef
    Then the response status is 404

  Scenario: labels are capped in length
    When POST /api/keys with a 200-char label
    Then the stored and listed label is at most 64 chars

  Scenario: revoking the last effective key is allowed
    Given auth.keys contains only sk-orouta-alice
    When DELETE /api/keys/{id of the TOML key}
    Then the response status is 200
    And the proxy is open until a new key is configured

  Scenario: mutations refuse an open proxy
    Given auth.keys is empty
    When POST /api/keys
    Then the response status is 403
    And DELETE /api/keys/{id} returns 403

  Scenario: corrupt overlay shows an error instead of TOML keys
    Given the overlay file contains garbage
    When GET /keys
    Then the body shows an overlay error banner
    And the body does not list the TOML keys as active
    And POST /api/keys returns 500

  Scenario: status page links to the keys page
    When GET /status with a valid key
    Then the header nav links to /keys
    And GET /keys links back to /status

  Scenario: status.json does not include keys
    Given a key was created
    When GET /status.json with a valid key
    Then the body has no keys field
