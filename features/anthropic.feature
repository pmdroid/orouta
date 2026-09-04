Feature: anthropic messages dialect

  Scenario: text request maps to ollama chat
    Given model claude-sonnet maps to desk with upstream_model llama3:70b
    When POST /v1/messages with system, max_tokens, and user text
    Then desk receives POST /api/chat
    And the first message is system
    And options.num_predict is set
    And the model sent upstream is llama3:70b

  Scenario: non-stream response translation
    When POST /v1/messages stream false
    Then content[0].text is the ollama message content
    And the response model is the client model name

  Scenario: stream deltas
    When POST /v1/messages stream true
    Then NDJSON content becomes content_block_delta events
    And a message_stop event is present

  Scenario: image body is rejected
    When POST /v1/messages with an image content block
    Then the response status is 400
    And no upstream is called

  Scenario: tools body is rejected
    When POST /v1/messages with tools
    Then the response status is 400
    And no upstream is called

  Scenario: unknown model is 404
    When POST /v1/messages with model does-not-exist
    Then the response status is 404
    And no upstream is called
