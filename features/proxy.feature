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

  Scenario: unknown pull name is 404
    When POST /api/pull with name does-not-exist
    Then the response status is 404
    And no host received /api/pull

  Scenario: pull uses config host when not listed yet
    Given [[model]] gemma4 maps to desk
    And no host lists gemma4 in /api/tags
    When POST /api/pull with name gemma4
    Then desk received /api/pull

  Scenario: tags and models come from hosts
    Given home /api/tags includes llama3:latest
    And desk /api/tags includes claude-sonnet
    When GET /api/tags
    And GET /v1/models
    Then both lists include those names
