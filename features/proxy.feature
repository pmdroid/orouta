Feature: ollama host proxy

  Scenario: chat routes llama3 to home only
    Given model llama3 maps to upstream home
    When POST /api/chat with model llama3
    Then home received the request
    And desk received no request

  Scenario: openai chat completions byte-forward
    Given model llama3 maps to upstream home with api_key
    When POST /v1/chat/completions with model llama3 and a client Bearer token
    Then the upstream Authorization is the config api_key
    And the client key is not forwarded

  Scenario: stream body matches upstream
    When POST /api/chat with model llama3 against a streaming upstream
    Then the concatenated response body equals the upstream bytes

  Scenario: unknown inference model is 404
    When POST /api/chat with model does-not-exist
    Then the response status is 404
    And no upstream is called

  Scenario: unknown pull name uses default upstream
    When POST /api/pull with name does-not-exist
    Then home received the request

  Scenario: tags and models are synthetic
    When GET /api/tags
    And GET /v1/models
    Then both lists come from config
    And no upstream is called
