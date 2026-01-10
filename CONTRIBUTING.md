Contributing to Floresta
==============================

The development of Floresta is a community effort and welcomes contributions from anyone. We are excited you are interested in helping us bringing sovereign and private self-custody to everyone.

We welcome contributions in many forms, including bug reports, feature requests, code contributions, and documentation improvements. From any contributors
with any level of experience or expertise. We only ask that you respect others and follow the process outlined in this document.

Communications Channels
-----------------------

The primary communication channel is the [GitHub repository](https://github.com/getfloresta/floresta). We also have Discord server, where you can ask questions, discuss features, and get help. You can join the server by clicking [here](https://discord.gg/5Wj8fjjS93).

Contribution Workflow
---------------------

The contribution workflow is designed to facilitate cooperation and ensure a high level of quality in the project. The process is as follows:

To contribute a patch, the workflow is as follows:

  1. Fork Repository
  2. Create topic branch
  3. Commit patches

### Commits

In general commits should be atomic and diffs should be easy to read.
For this reason do not mix any formatting fixes or code moves with actual code
changes. Further, each commit, individually, should compile and pass tests, in
order to ensure git bisect and other automated tools function properly.

When adding a new feature ensure that it is covered by functional tests where possible.

When refactoring, structure your PR to make it easy to review and don't
hesitate to split it into multiple small, focused PRs.

The Minimum Supported Rust Version is **1.81.0** (enforced by our CI).

Commits should cover both the issue fixed and the solution's rationale.


These [guidelines](https://chris.beams.io/posts/git-commit/) should be kept in mind. Commit
messages follow the ["Conventional Commits 1.0.0"](https://www.conventionalcommits.org/en/v1.0.0/) to make commit histories easier to read by humans and automated tools. The types of commits we use are:

  - bench: benchmark related changes
  - chore: maintenance tasks
  - ci: continuous integration
  - docs: documentation changes
  - feat: new feature
  - fix: bug fix
  - fuzz: adding a new fuzz target or fixing an existing one
  - perf: code change that improves performance
  - refactor: code change that neither fixes a bug nor adds a feature
  - style: formatting, missing semi colons, etc; no code change
  - test: adding missing tests or correcting existing tests

For `scope`, consider using one of the following where applicable:

  - chain
  - consensus
  - cli
  - common
  - deps
  - electrum
  - filters
  - florestad
  - integration, functional, unit (to be used in test commits)
  - mempool
  - node
  - rpc
  - wallet
  - wire

It is required to [GPG sign](https://docs.github.com/en/authentication/managing-commit-signature-verification/signing-commits) your commits.

Peer review
-----------

To make sure our code has the highest quality and is maintainable for posterity, we have a thorough peer review process, where pull requests need to be reviewed by at least one maintainer, and must not have any outstanding comment from regular contributors.

We welcome everyone to review and give their feedback on changes to Floresta. The conventions on how to communicate in a code review are based on [Bitcoin Core](https://github.com/bitcoin/bitcoin/blob/v23.0/CONTRIBUTING.md#peer-review)

### Conceptual Review

A review can be a conceptual review, where the reviewer leaves a comment:

- Concept (N)ACK: "I do (not) agree with the general goal of this pull request",
- Approach (N)ACK: Concept (N)ACK, but "I do (not) agree with the approach of this change".

A NACK needs to include a rationale why the change is not worthwhile. NACKs without accompanying reasoning may be disregarded.

### Code Review
After conceptual agreement on the change, code review can be provided. A review begins with ACK BRANCH_COMMIT, where BRANCH_COMMIT is the top of the PR branch, followed by a description of how the reviewer did the review. The following language is used within pull request comments:

"I have tested the code", involving change-specific manual testing in addition to running the unit, functional, or fuzz tests, and in case it is not obvious how the manual testing was done, it should be described;
"I have not tested the code, but I have reviewed it and it looks OK, I agree it can be merged";
A "nit" refers to a trivial, often non-blocking issue.
Project maintainers reserve the right to weigh the opinions of peer reviewers using common sense judgement and may also weigh based on merit. Reviewers that have demonstrated a deeper commitment and understanding of the project over time or who have clear domain expertise may naturally have more weight, as one would expect in all walks of life.

Where a patch set affects consensus-critical code, the bar will be much higher in terms of discussion and peer review requirements, keeping in mind that mistakes could be very costly to the wider community. This includes refactoring of consensus-critical code.

Where a patch set proposes to change the Bitcoin consensus, it must have been discussed extensively on the mailing list and IRC, be accompanied by a widely discussed BIP and have a generally widely perceived technical consensus of being a worthwhile change based on the judgement of the maintainers.

Coding Conventions
------------------

There's a few rules to make sure the code is readable and maintainable. Most of them are checked by `cargo-fmt` and `clippy`, and are enforced by CI. You can run locally `cargo +nightly fmt && cargo +nightly clippy --all` or, if you have the [Just Command Runner](https://github.com/casey/just) you might use `just lint`.

For the sake of clarity, please use an empty line between items, in both the Python and Rust code. Some examples

```rust
///! Awesome module
///!
///! Awesome description:
///! I do neat things.
///!
///! # Example
///! If it's a public module, you should have an example here.

/// Awesome Foo struct
///
/// Here's how it works
pub struct Foo {
  /// Some awesome comment
  bar: u32,

  /// Another awesome comment
  foo: u32
}

impl Foo {
  /// Creates a new Foo
  pub fn new() {
    todo!()
  }

  /// Docs for my awesome method
  pub fn some_awesome_method() {
      todo!()
  }
}
```

Python version

```python
"""Awesome module comment"""

class Foo:
   """Some awesome comment"""

      def func(self):
            """Some awesome comment"""
            pass
```

Only make `pub` things that needs to be pub. This codebase is meant to be used as a library, we don't want users peeking on our internals.

If you need an attribute, use the attribute **before** the docstring. Example

```rust
#[derive(Debug, Default)]
/// Some comment
enum Foo {
  #[Default]
  /// The Bar case
  Bar
  /// The Car case
  Car,
}
```

All public items must be documented. We adhere to the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/about.html) with respect to documentation
The library is written using safe rust. Special consideration must be given to code which proposes an exception to the rule.

All new features require testing. Tests should be unique and self-describing. If a test is in development or is broken or no longer useful, then a reason should be given for adding the `#[ignore]` attribute.

If you have `just`, we have a script that performs all the checks we do on CI (test, linting, docker...) use `just pcc` (pre commit check) before pushing your changes.

When it comes error handling, we prefer exact and meaningful error handling to deliver consumers(developers and users) an accurate error that describes exactly what happened wrong.

Instead of:

```rust
/// A function that will give the same error for totally different purposes.
pub fn validate_block_time(
    block_timestamp: u32,
    mtp: u32,
    time: impl NodeTime,
) -> Result<(), BlockValidationErrors> {
    if mtp > block_timestamp && block_timestamp > (time.get_time().sub(2 * HOUR)) {
        return Err(BlockValidationErrors::InvalidBlockTimestamp);
    }

    Ok(())
}
```

prefer:

```rust
/// A function that exactly explains what can go wrong and why in its code and in return type
pub fn validate_block_time(
    block_timestamp: u32,
    mtp: u32,
    time: impl NodeTime,
) -> Result<(), BlockValidationErrors> {
    let its_too_old = mtp > block_timestamp;
    let its_too_new = block_timestamp > (time.get_time().sub(2 * HOUR));
    if its_too_old {
        return Err(BlockValidationErrors::BlockTooNew);
    }
    if its_too_new {
        return Err(BlockValidationErrors::BlockTooNew);
    }
    Ok(())
}
```

Documentation for RPC
---------------------

We aim on having a good documentation and CLI `help` command.

To achieve this, we use the `rustdoc` tool, which generates documentation from Rust source code comments. We also use the `clap` library to generate CLI help and usage information directly from the code.

Please, always create a new RPC documentation under the specified [directory](/doc/rpc) and implement using the following syntax on the method command definition:

```rust
#[doc = include_str!("../../../doc/rpc/command.md")]
#[command(name = "command_name",
    about = "Write a short description of the command",
    long_about = Some(include_str!("../../../doc/rpc/command.md")),
    disable_help_subcommand = true)]
MethodStruct{
    arg1: type,
    arg2: type,
    arg3: type,
}
```

Example:
```rust
#[doc = include_str!("../../../doc/rpc/addnode.md")]
#[command(name = "addnode",
    about = "Attempts to add or remove a node from the list of addnodes",
    long_about = Some(include_str!("../../../doc/rpc/addnode.md")),
    disable_help_subcommand = true)]
AddNode {
    node: String,
    command: AddNodeCommand,
    v2transport: Option<bool>,
},
```

To generate the man pages for the RPC commands, follow the instructions outlined [here](doc/RPC_man/README.md).

Security
--------

Given the critical nature of Floresta as a node implementation, we take security very seriously. For information on how to report security vulnerabilities, please see our [Security Policy](SECURITY.md).

Testing
-------

We expect to have 100% test coverage for critical parts, and a decent level of coverage for everything. We have a few types of tests:

  - Unit: Those tests specific parts of the code, and are usually written in Rust. You can run them using `cargo test`. Ideally, every API-exposed function should have their own unity test.
  - Functional: Tests the behavior of the running program, intended to check whether the codebase as a whole works as expected. They are either written in Rust or Python
  - Integration: Checks if `Floresta` works well with other projects, like `Bitcoin Core`, `utreexod` and `Electrum` (for the electrum server). Mainly written in Python.

Release
-----------

Once a maintainer and the contributors decide we have a stable enough `master` with sufficient features, we will create a new branch at that point. From this point, all new changes will go in the next release. The release branch (named after the version, i.e. 0.8.0) will only accept bugfixes and backports. After sufficient testing and making sure we don't have bugs left, this branch will be released by one of the maintainers.

The release will have pre-built binaries available on github's asset page. They **must** be GPG signed, and have a list of hashes for each asset.

If we find bugs on a release, the fix may be backported and a new minor release may be released. This is done by merging fixes on top of the release branch. And then performing another release on that branch.


If you have any questions, related to this process or the codebase in general. Don't hesitate to reach us out, we are happy to help newcomers in their amazing journey. Overall, have fun :)
