Feature: host health visibility

  Scenario: model on a down host is 503
    Given home is unreachable
    When POST /api/chat with model llama3
    Then the response status is 503
    And the body says host unavailable for host home

  Scenario: recovery is picked up within one request
    Given home is unreachable
    When POST /api/chat with model llama3
    Then the response status is 503
    And home recovers
    When POST /api/chat with model llama3 again
    Then the second response status is 200

  Scenario: known model on a down host is 503 and recovers within one request
    Given home /api/tags includes llama3
    When POST /api/chat with model llama3
    Then the response status is 200
    And home stops accepting connections
    When POST /api/chat with model llama3
    Then the response status is 502 naming host home
    And a further POST /api/chat with model llama3 is 503 host unavailable
    And home accepts connections again
    When POST /api/chat with model llama3 again
    Then the final response status is 200

  Scenario: forward connection error names the host
    Given the first host is unreachable
    When a request without a model is forwarded
    Then the response status is 502
    And the body names host dead

  Scenario: healthy host still routes while another is down
    Given home is unreachable
    And desk /api/tags includes mistral
    When POST /api/chat with model mistral
    Then the response status is 200

  Scenario: unknown model with all hosts healthy is 404
    Given home /api/tags includes llama3
    And desk /api/tags includes mistral
    When POST /api/chat with model does-not-exist
    Then the response status is 404
    And the body says unknown model

  Scenario: anthropic messages names a down host
    Given home /api/tags includes llama3
    When POST /v1/messages with model llama3
    Then the response status is 200
    And home stops accepting connections
    When POST /v1/messages with model llama3
    Then the response status is 502 naming host home
    And a further POST /v1/messages with model llama3 is 503 host unavailable
    And home accepts connections again
    When POST /v1/messages with model llama3 again
    Then the final response status is 200

  Scenario: anthropic unknown model on a down host is 503
    Given home is unreachable
    When POST /v1/messages with model does-not-exist
    Then the response status is 503
    And the body says host unavailable for host home
