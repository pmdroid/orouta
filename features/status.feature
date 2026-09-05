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

  Scenario: tailscale serving shows chip and link
    Given orouta's tailscale self is box.tail-scale.ts.net and serving
    When GET /status with a valid key
    Then the body contains an accent TAILSCALE chip linking to https://box.tail-scale.ts.net
    And /status.json has tailscale self, tailnet, online true, serving true and the url

  Scenario: tailscale offline shows dimmed chip
    Given orouta's tailscale self is box.tail-scale.ts.net but offline
    When GET /status with a valid key
    Then the body contains a dimmed "TAILSCALE · offline" chip
    And /status.json has tailscale online false, serving false and a null url

  Scenario: tailscale online but not serving shows dimmed chip
    Given orouta's tailscale self is box.tail-scale.ts.net and online but / does not answer
    When GET /status with a valid key
    Then the body contains a dimmed "TAILSCALE · no serve" chip
    And /status.json has tailscale online true, serving false and a null url

  Scenario: tailscale CLI missing or not in a tailnet renders nothing
    Given tailscale status is unavailable
    When GET /status with a valid key
    Then the body contains no TAILSCALE chip
    And /status.json has tailscale null

  Scenario: streamed chat response records tps sample
    Given home /api/tags includes llama3:latest
    And home /api/chat streams a final chunk with eval_count 136 and eval_duration 3459000000
    When POST /api/chat with model llama3
    Then /status.json has tps for model llama3 with avg 39.3, last 39.3, prompt 40.0 and samples 1

  Scenario: non-streaming chat response records tps sample
    Given home /api/tags includes llama3:latest
    And home /api/chat returns json with eval_count 88 and eval_duration 2281000000
    When POST /api/chat with model llama3
    Then /status.json has tps for model llama3 with avg 38.6 and no prompt tps

  Scenario: non-eval response is forwarded unchanged and records no sample
    Given home /api/chat returns plain text with no eval fields
    When POST /api/chat with model llama3
    Then the response body is byte-identical to the upstream body
    And /status.json has no tps for home

  Scenario: tps window keeps the latest 50 samples per host and model
    Given home /api/chat streams 60 responses with eval_count 10 and eval_duration 1000000000
    When POST /api/chat with model llama3 60 times
    Then /status.json has tps for model llama3 with samples 50

  Scenario: host with loaded models reports vram
    Given home /api/ps reports gemma4:e2b with size_vram 7801585920
    When GET /status.json with a valid key
    Then home has vram loaded_bytes 7801585920 with models gemma4:e2b
    And desk has vram null

  Scenario: vram stays empty when the host does not answer /api/ps
    Given home /api/ps is unreachable
    When GET /status.json with a valid key
    Then home has vram null
