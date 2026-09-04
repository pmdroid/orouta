Feature: ollama host proxy

  Scenario: chat routes llama3 to the host that lists it
    Given home /api/tags includes llama3:latest
    And desk /api/tags includes mistral
    When POST /api/chat with model llama3
    Then home received /api/chat
    And desk did not receive /api/chat

  Scenario: openai chat completions byte-forward
    Given home /api/tags includes llama3
    And home has api_key
    When POST /v1/chat/completions with model llama3 and a client Bearer token
    Then the upstream Authorization is the config api_key
    And the client key is not forwarded

  Scenario: stream body matches upstream
    When POST /api/chat with model llama3 against a streaming upstream
    Then the concatenated response body equals the upstream bytes

  Scenario: unknown inference model is 404
    When POST /api/chat with model does-not-exist
    Then the response status is 404
    And no host received /api/chat

  Scenario: pull with unknown name forwards to selected host
    When POST /api/pull?host=home with name does-not-exist
    Then home received /api/pull
    And desk did not receive /api/pull

  Scenario: pull with host param forwards to that host
    When POST /api/pull?host=desk with name mistral
    Then desk received /api/pull and streams the ndjson progress body
    And home did not receive /api/pull

  Scenario: pull_host config selects the download host
    Given pull_host = "home"
    When POST /api/pull with name llama3
    Then home received /api/pull
    And desk did not receive /api/pull

  Scenario: host param overrides pull_host
    Given pull_host = "home"
    When POST /api/pull?host=desk with name llama3
    Then desk received /api/pull
    And home did not receive /api/pull

  Scenario: pull with multiple hosts and no selection is 400
    When POST /api/pull with name llama3
    Then the response status is 400
    And the error names the available host ids
    And no host received /api/pull

  Scenario: pull with unknown host param is 400
    When POST /api/pull?host=nope with name llama3
    Then the response status is 400
    And no host received /api/pull

  Scenario: pull on a single host needs no selection
    Given only one upstream is configured
    When POST /api/pull with name llama3
    Then home received /api/pull

  Scenario: tags and models come from hosts
    Given home /api/tags includes llama3:latest
    And desk /api/tags includes claude-sonnet
    When GET /api/tags
    And GET /v1/models
    Then both lists include those names
