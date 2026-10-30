# AI policy

Nightcap takes contributions made with AI tools, such as a chat model or a coding agent, as long as a person stands behind every one. This policy is adapted from [Sigstore's](https://github.com/sigstore/.github/blob/main/AI_POLICY.md).

In short:

- Say how you used AI, in the issue or the pull request.
- Understand everything you submit, and be ready to explain it.
- Put your own name to your commits, and to your replies.

## You're responsible for what you submit

Whatever you open, an issue or a pull request, is yours, whether you wrote it by hand or a tool generated it. Read all of it before you submit it, and understand both your change and the code it touches. A pull request whose author can't explain it is closed.

Respect the maintainer's time: don't send work you haven't reviewed yourself.

## Say how you used AI

Every issue and pull request says, in its description, whether AI helped make it, and how: which tool, and for what, such as writing code, writing tests, finding a bug or drafting the description. The description is the one place for it: the code and the commits stay as they would be without it. An issue or pull request that looks generated and says nothing may be closed without review.

## Speak for yourself

In issues, pull requests and reviews, write your own replies. Don't let an AI answer questions about your change or post comments for you. A tool can help you phrase something; the thinking and the answer are yours.

## Before you open a pull request

- **Open an issue first,** unless the change is small: a typo, a documentation fix, an obvious bug. Say why the change is needed and roughly how it would go.
- **Keep it small and about one thing.** A pull request over about 1,000 changed lines will be asked to split.
- **Commit as yourself.** Your commits name you alone, and you stand behind each one: a commit carries no `Co-Authored-By`, `Assisted-by` or similar line for an AI tool.
- **Test it.** Run `check` (see [AGENTS.md](AGENTS.md)), and add tests for what you change. Understand the tests as well as the code.
- **Follow the project's conventions:** [AGENTS.md](AGENTS.md), the style guides in [`docs/style/`](docs/style/), the glossaries and the decision records in [`docs/adr/`](docs/adr/). AGENTS.md is written for people and coding agents alike.
- **Write plainly.** A description, commit message or comment says what changed and why, in a few clear sentences. Padding and generated filler get a pull request sent back.

## What gets closed

- An issue or pull request that looks generated and doesn't say how AI was used.
- A pull request whose commits credit an AI tool, and aren't rewritten under your name alone when asked.
- A change its author can't explain, or one without tests.
- A large or unfocused change.
- A change that weakens what Nightcap protects: its signing, its socket, what it keeps or sends (which the [privacy policy](site/src/Privacy.tsx) describes), or anything else a user trusts it with.
- Replies written by an AI.

## Licence

Contributions are under the [MIT licence](LICENSE). You must have the right to submit what you send: generated code that reproduces someone else's work is no different from copying it.
