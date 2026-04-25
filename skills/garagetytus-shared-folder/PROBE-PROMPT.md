# Probe prompts — does this pod's agent know about shared folders?

After pasting `AGENT-PROMPT.md` (or running
`bin/garagetytus-agent-prompt`) into a pod's chat window, paste
ONE of the prompts below to verify the agent absorbed it. The
response tells you whether the setup landed.

## Probe 1 — quick smoke test (recommended)

Paste this:

```
Quick check on our shared file system:

1. Which S3 bucket(s) do you have access to, and what's your
   pod identity?
2. Show me the exact `aws s3 cp` command you'd run to save a
   file called "hello-sebastian.md" so I see it on my Mac.
3. Show me the exact command you'd run to list files I've put
   in from-mac/ for you.

If shared-folder access isn't set up yet, say so plainly — do
NOT guess endpoints or credentials.
```

**Pass criteria** — the agent's response should:

- Name the actual bucket (e.g. `testbucket`, `work`) and pod
  identity (e.g. `wannolot-02`) — NOT a placeholder.
- Use the endpoint `http://10.42.42.1:3900` literally.
- Use the `--profile garagetytus` flag (or equivalent inline
  `--access-key` arguments).
- Prefix its output file with `from-<pod-id>/` (e.g.
  `s3://testbucket/from-wannolot-02/hello-sebastian.md`) — NOT
  bare bucket root, NOT `from-mac/`.
- For the LIST: `aws s3 ls s3://<bucket>/from-mac/ ...`

**Fail tells:**
- Mentions `~/Documents/` or local paths (it doesn't have those).
- Hallucinates an endpoint like `s3.amazonaws.com` or `localhost`.
- Says "I don't have S3 access" — means the AGENT-PROMPT didn't
  land or was lost from context.
- Wraps everything in placeholders like `<your-bucket>` instead
  of using the actual bucket name.

## Probe 2 — actually do it (live round-trip)

If Probe 1 passes, paste this to confirm end-to-end works:

```
Do this now and report back:

1. Save a file called "probe-from-pod.md" with the content
   "I'm alive at <current UTC time> from <your pod id>" to
   the shared bucket.
2. Run `aws s3 ls` to confirm it landed.
3. Tell me the full s3:// URL you used so I can verify from
   my Mac.
```

Then on Mac, verify:

```bash
rclone --config ~/.config/rclone/rclone.conf cat \
    garagetytus:<bucket>/from-<pod-id>/probe-from-pod.md
```

If you see the timestamp the agent emitted, the shared-folder
path is alive end-to-end from this agent.

## Probe 3 — multi-bucket routing

Only if the pod has been granted multiple buckets. Paste:

```
You have access to multiple shared folders. Without running
any commands, tell me:

1. What buckets do you see in /etc/garagetytus.shared.json (or
   via aws s3api list-buckets)?
2. If I asked you to "save the meeting notes from today",
   which bucket would you pick — and why?
3. If I asked you to "send a file to all my pods", which
   bucket?

If you'd ask me before guessing, say that — that's the right
answer when the routing isn't obvious from the task.
```

**Pass criteria** — the agent should either give a confident
mapping (work/personal/agent-results matched to file types) OR
explicitly ask Sebastian to clarify. **Guessing silently is a
fail** — wrong-bucket = wrong-audience.

## Probe 4 — failure-mode check

Paste this to verify the agent doesn't loop on errors:

```
Hypothetical: you try to PUT a file and get
`dial tcp 10.42.42.1:3900: connection refused`.

What do you do?
```

**Pass:** "Tell Sebastian; the WireGuard tunnel or droplet
forwarder is down. Don't retry blindly."

**Fail:** Any answer that includes "retry with backoff" without
also surfacing the issue to Sebastian first.

## When all probes pass

The pod's agent has the shared-folder skill loaded correctly.
Drop a file in your Mac shared folder, run rclone bisync, and
ask the agent "did you get the file I just sent?". The agent
should `aws s3 ls` and confirm.

## When a probe fails

Re-paste the AGENT-PROMPT. If it still fails:

- Check that the agent's runtime supports preserved system
  prompts across messages (some chat UIs drop them).
- Check that `/etc/garagetytus.env` exists in the pod (some
  pods get the agent prompt but not the env file — they'll
  still work via the inline credentials in the prompt itself).
- Check that the WireGuard tunnel is up
  (`tytus status` from Mac, or
  `curl -s -o /dev/null -w "%{http_code}\n" http://10.42.42.1:3900/`
  from inside the pod).
