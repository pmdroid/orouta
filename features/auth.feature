Feature: client auth

  Scenario: Bearer token accepted
    Given auth.keys contains sk-orouta-alice
    When GET /api/tags with Authorization Bearer sk-orouta-alice
    Then the response status is 200

  Scenario: x-api-key accepted
    Given auth.keys contains sk-orouta-alice
    When GET /api/tags with x-api-key sk-orouta-alice
    Then the response status is 200

  Scenario: missing key rejected
    Given auth.keys contains sk-orouta-alice
    When GET /api/tags with no auth header
    Then the response status is 401

  Scenario: wrong key rejected
    Given auth.keys contains sk-orouta-alice
    When GET /api/tags with Authorization Bearer sk-wrong
    Then the response status is 401

  Scenario: empty keys is open
    Given auth.keys is empty
    When GET /api/tags with no auth header
    Then the response status is 200
