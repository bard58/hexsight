# Security policy

## Supported versions

Only the latest release gets security fixes.

## Report a vulnerability

Do not open a public issue for a security problem.

1. Use the private report form: open the **Security** tab of this repository and select **Report a vulnerability**.
2. If the form is not there, write to drops.grizzly_67@icloud.com with the subject "Hexsight security".
3. Say what you found, the version, and the steps to repeat it.

The maintainer works on this project in free time. Expect an answer when the maintainer can give one. After a fix, the maintainer publishes a release and credits you, if you want.

## What counts

Examples:

- Code from a download or a data file runs on the PC.
- Hexsight writes outside its own folders and the League item set folder.
- Hexsight sends the League Client password, your player ID, or your screen to the network.
- A weakness in the install script or the release files.

## What does not count

- Windows SmartScreen or antivirus warnings for an unsigned exe. Compare the SHA-256 hash with the release file.
- The `testpass` password and the test certificate of the mock League Client in `tools\mock`. They are for local tests only.
- Requests for features that read game memory, inject code, or send input to the game. The project does not do these things.

## Scope note

Hexsight talks to the League Client on 127.0.0.1 with a self-signed certificate and skips the certificate check for these two local addresses only. This is by design.
