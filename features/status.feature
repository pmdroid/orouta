Feature: status page

  Scenario: HTML page lists hosts and models
    Given home /api/tags includes llama3:latest
    And desk /api/tags includes mistral
    When GET /status with a valid key
    Then the response status is 200
    And the response content type is text/html
    And the body contains each host id, base url and model name

  Scenario: JSON page reports per-host state
    When GET /status.json with a valid key
    Then the response status is 200
    And each host has base_url, reachable, latency_ms, models, requests_total, errors_total, in_flight and last_error

  Scenario: status routes require a key
    Given auth.keys contains sk-orouta-alice
    When GET /status with no auth header
    Then the response status is 401
    And GET /status.json with no auth header returns 401

  Scenario: empty keys is open
    Given auth.keys is empty
    When GET /status with no auth header
    Then the response status is 200

  Scenario: forwarded chat updates counters
    Given home /api/tags includes llama3:latest
    When POST /api/chat with model llama3
    Then home requests_total is 1 and errors_total is 0
    And desk requests_total is 0

  Scenario: upstream error is counted and recorded
    When POST /api/chat against a failing upstream with model llama3
    Then home errors_total is 1
    And home last_error mentions the upstream failure

  Scenario: messages request updates counters
    Given home /api/tags includes llama3:latest
    When POST /v1/messages with model llama3
    Then home requests_total is 1 and errors_total is 0
