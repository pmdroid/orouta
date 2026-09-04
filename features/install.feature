Feature: curl install from GitHub releases

  Scenario: linux amd64 target
    Given uname is linux x86_64
    When install.sh --print-target
    Then the target is x86_64-unknown-linux-gnu

  Scenario: mac arm target
    Given uname is darwin arm64
    When install.sh --print-target
    Then the target is aarch64-apple-darwin

  Scenario: latest release url
    Given OROUTA_VERSION is latest
    When install.sh --print-url
    Then the url is https://github.com/pmdroid/orouta/releases/latest/download/orouta-<target>

  Scenario: pinned release url
    Given OROUTA_VERSION is v0.1.0
    When install.sh --print-url
    Then the url is https://github.com/pmdroid/orouta/releases/download/v0.1.0/orouta-<target>
