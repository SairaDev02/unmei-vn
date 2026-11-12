# Security policy

This page explains how to report a security vulnerability in Unmei privately, and which versions receive security fixes.

## Supported versions

Unmei is in beta through 0.x. Only the commit at the tip of `main`, the development branch, receives security fixes, and a fix reaches users in a new 0.x release. Other branches, tags and earlier releases receive no fixes.

## Reporting a vulnerability

**Do not report a security vulnerability through a public GitHub issue.** Use GitHub's private vulnerability reporting instead:

1. Go to the **Security** tab of this repository.
2. Select **Report a vulnerability**.
3. Provide a description, reproduction steps and any affected components.

## What to expect

- **Acknowledgment:** we acknowledge your report within 7 days of receiving it.
- **Status updates:** we send a status update at least every 7 days, counted from receipt, until the vulnerability is resolved.
- **Coordinated disclosure:** we ask you to keep the report private until a fix is released. We aim to resolve a reported vulnerability within 90 days of receiving the report.

## Scope

In scope: anything in this repository that could compromise the confidentiality, integrity or availability of systems that run this code or of the people who use it.

Out of scope:

- vulnerabilities in dependencies that this code cannot reach (report those to the dependency's maintainers);
- social engineering attacks.
