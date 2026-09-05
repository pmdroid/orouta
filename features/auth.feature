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

  Scenario: browser navigation without auth gets the shell but data 401s
    Given auth.keys contains sk-orouta-alice
    When GET /status as a browser with no auth
    Then the response status is 200 and the body is the SPA shell
    And GET /status.json with no auth returns 401

  Scenario: login with valid key sets HttpOnly session cookie
    Given auth.keys contains sk-orouta-alice
    When POST /api/login with key sk-orouta-alice
    Then the response sets an HttpOnly orouta_key cookie

  Scenario: login with wrong key is rejected
    Given auth.keys contains sk-orouta-alice
    When POST /api/login with key sk-wrong
    Then the response status is 401

  Scenario: session cookie authorizes UI and API
    Given a browser session logged in as sk-orouta-alice
    When GET /status with the session cookie
    Then the response status is 200

  Scenario: revoked key kills its session
    Given a browser session logged in as sk-orouta-alice
    When sk-orouta-alice is revoked
    Then GET /status with the session cookie still returns the shell
    And GET /status.json with the session cookie returns 401

  Scenario: login is refused on an open proxy
    Given auth.keys is empty
    When GET /login
    Then the response status is 200 and the body is the SPA shell
    And GET /api/keys returns an empty keys list
    And POST /api/login returns 401
