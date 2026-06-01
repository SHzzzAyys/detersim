# Security Policy

DeterSim is a deterministic simulation testing framework. It is not a network
service and the deterministic core should not touch real sockets, real files,
production credentials, wall-clock time, OS randomness, or threads.

## Supported Versions

The current supported line is the latest `main` branch and tagged beta releases.
Older alpha tags are kept for reproducibility but do not receive backported
security fixes.

## Security-Relevant Areas

Please report security issues for:

- GitHub Actions workflow or release supply-chain risks.
- Dependency confusion or unexpected external dependency introduction.
- Static HTML artifact escaping issues, including possible script injection.
- CLI path handling that can unexpectedly overwrite files outside the requested
  output directory.
- Repository metadata or packaging issues that could mislead users.

## Not Security Issues

These should use normal issue templates instead:

- Protocol bugs found by a DeterSim experiment.
- Same-seed divergence or determinism leaks.
- Checker false positives or false negatives.
- Missing benchmark coverage.
- Documentation overclaims.

## Reporting

If private reporting is unavailable, open a GitHub issue with a minimal
reproduction and avoid including secrets. DeterSim should not require secrets to
reproduce deterministic framework behavior.

Include:

- affected commit or tag
- exact command
- generated artifact if relevant
- expected vs actual behavior
- whether the issue involves deterministic core crates or only the CLI
