Feature: manage upstream hosts from the status UI

  Scenario: add a host via the JSON API
    Given home /api/tags includes llama3:latest
    When POST /api/hosts with id desk and base_url of desk
    Then the response status is 200
    And the response hosts list includes desk with disabled false
    And desk models appear in /api/tags after refresh
    And the overlay file records the added host

  Scenario: add rejects invalid or duplicate hosts
    When POST /api/hosts with a non-http base_url
    Then the response status is 400
    When POST /api/hosts with an existing id
    Then the response status is 400

  Scenario: api_key is never echoed back
    When POST /api/hosts with id desk and an api_key
    Then the response and /status.json contain api_key_set true but never the key value
    And the status page shows "api_key: set"

  Scenario: disable excludes host from catalog and probing
    Given home and desk are up
    When POST /api/hosts/desk/disable
    Then the response status is 200
    And /status.json marks desk disabled true
    And desk models disappear from /api/tags
    And the status page renders the desk row with DISABLED / not probed
    And the overlay file records desk as disabled

  Scenario: enable restores the host
    Given desk is disabled
    When POST /api/hosts/desk/enable
    Then desk models reappear in /api/tags

  Scenario: remove deletes a host via the overlay
    Given home and desk are up
    When DELETE /api/hosts/desk
    Then the response status is 200
    And desk disappears from /status.json and /api/tags
    And the overlay file records desk as removed

  Scenario: overlay wins over a TOML re-added host
    Given the overlay removed desk
    When desk is re-added to the TOML config and it reloads
    Then desk stays absent from /status.json

  Scenario: remove refuses while requests are in flight
    Given a chat request to home is in flight
    When DELETE /api/hosts/home
    Then the response status is 409
    When the chat finishes and DELETE /api/hosts/home runs again
    Then the response status is 200

  Scenario: mutations refuse an open proxy
    Given auth.keys is empty
    When POST /api/hosts
    Then the response status is 403
    And POST /api/hosts/{id}/disable, /enable and DELETE /api/hosts/{id} return 403

  Scenario: mutations require a valid key when keys are configured
    Given auth.keys contains sk-orouta-alice
    When POST /api/hosts with no auth header
    Then the response status is 401

  Scenario: mutations on an unknown host return 404
    When POST /api/hosts/ghost/disable
    Then the response status is 404
