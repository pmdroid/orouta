Feature: reload orouta.toml without restarting

  Scenario: upstream base_url change reroutes new requests
    Given home /api/tags includes llama3
    When orouta.toml changes home's base_url to desk
    And POST /api/chat with model llama3
    Then desk received /api/chat

  Scenario: upstream set change resets the catalog
    Given home /api/tags includes llama3
    And the catalog cached llama3 on home
    When orouta.toml adds desk and home no longer lists llama3
    And POST /api/chat with model llama3
    Then desk received /api/chat

  Scenario: invalid config keeps the old one
    Given home /api/tags includes llama3
    When orouta.toml becomes invalid
    And POST /api/chat with model llama3
    Then home received /api/chat
    And desk did not receive /api/chat

  Scenario: auth key change takes effect
    Given the configured key is sk-orouta-alice
    When orouta.toml changes the keys to sk-orouta-new
    Then a request with sk-orouta-alice is unauthorized
    And a request with sk-orouta-new is authorized

  Scenario: bind host and port changes are ignored
    When orouta.toml changes host and port
    Then the proxy keeps serving on the original listener
