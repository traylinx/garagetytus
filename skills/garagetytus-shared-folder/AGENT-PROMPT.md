# Drop-in agent prompt — paste into the chat window

This is a self-contained system prompt fragment you paste into
any AI chat window (Claude Code, ChatGPT, OpenClaw, Hermes,
gemini, opencode, anything) running inside or alongside a tytus
pod. After pasting, the agent knows:

1. It has access to a shared S3 bucket
2. The endpoint and credentials
3. How to PUT, GET, and LIST files
4. The naming convention so it doesn't overwrite Mac's or
   another pod's files

## How to use

1. On the droplet, run `garagetytus bucket grant <bucket> --to <pod>`
   to mint creds for the pod. Output gives `access_key_id`
   and `secret_access_key`.
2. Copy the template below.
3. Replace the four `<…>` placeholders with real values (bucket
   name, pod identity, access key, secret).
4. Paste into the agent's chat window as the FIRST message
   (or set as system prompt if the runtime supports it).

That's it. The agent now has the recipe for sharing files.

---

## Template — copy from here ↓↓↓

> **Tytus pods ship boto3 (Python) but NOT the aws CLI.** This
> template uses boto3 because that's what actually works in the
> pod environment. If your target environment has the aws CLI
> (e.g. you're pasting this into a chat for a Mac-side agent),
> see the alternative aws-CLI block below.

```
You have access to a shared file system through a Garage S3
bucket on Sebastian's droplet. Use this when you need to save
a file Sebastian (or another pod) will see, or read a file
they've added for you.

ENDPOINT:    http://10.42.42.1:3900
BUCKET:      <bucket-name>
YOUR ID:     <pod-name>           # e.g. wannolot-02 — used in file prefixes
ACCESS KEY:  <access-key-id>
SECRET KEY:  <secret-access-key>
REGION:      garage

NOTE: This pod has boto3 (Python) installed but NOT the aws CLI.
Use the Python recipes below — do not invent aws-cli commands.

# How to PUT a file (Sebastian or other pods will see it):
python3 - <<'PY'
import boto3
from botocore.config import Config
s3 = boto3.client("s3",
    endpoint_url="http://10.42.42.1:3900",
    aws_access_key_id="<access-key-id>",
    aws_secret_access_key="<secret-access-key>",
    region_name="garage", config=Config(s3={"addressing_style": "path"}))
with open("./your-file.md", "rb") as f:
    s3.put_object(Bucket="<bucket-name>",
                  Key="from-<pod-name>/your-file.md",
                  Body=f.read())
print("OK")
PY

# How to LIST what's in the bucket:
python3 - <<'PY'
import boto3
from botocore.config import Config
s3 = boto3.client("s3", endpoint_url="http://10.42.42.1:3900",
    aws_access_key_id="<access-key-id>",
    aws_secret_access_key="<secret-access-key>",
    region_name="garage", config=Config(s3={"addressing_style": "path"}))
for obj in s3.list_objects_v2(Bucket="<bucket-name>").get("Contents", []):
    print(f"{obj['Key']:50} {obj['Size']:>8} bytes")
PY

# How to GET a file Sebastian sent you:
python3 - <<'PY'
import boto3
from botocore.config import Config
s3 = boto3.client("s3", endpoint_url="http://10.42.42.1:3900",
    aws_access_key_id="<access-key-id>",
    aws_secret_access_key="<secret-access-key>",
    region_name="garage", config=Config(s3={"addressing_style": "path"}))
data = s3.get_object(Bucket="<bucket-name>",
                     Key="from-mac/instructions.md")["Body"].read()
open("./instructions.md", "wb").write(data)
print(f"got {len(data)} bytes")
PY

NAMING CONVENTION inside this bucket:
- Files YOU produce → prefix with from-<pod-name>/
- Files Sebastian sent you → look in from-mac/
- Files for ALL pods in this bucket → prefix with broadcast/

WHAT NOT TO PUT IN THIS BUCKET:
- Secrets / API keys / passwords (this bucket is shared)
- Multi-GB blobs without explicit user approval (disk is finite)
- Conversational memory or chat history (use Brain instead)

WHEN UNSURE which bucket to use, or whether something belongs
in S3 vs Brain, ASK SEBASTIAN. Don't guess — wrong bucket =
wrong audience.

If the endpoint refuses connections, tell Sebastian; don't
loop on retries.
```

## Alternative — aws CLI flavor (Mac-side agents, custom pods)

If the target has the aws CLI installed (e.g. a Mac-side agent,
or a pod image that ships awscli), use this block instead. Same
substitutions: `<bucket-name>`, `<pod-name>`, `<access-key-id>`,
`<secret-access-key>`.

```
ENDPOINT:    http://10.42.42.1:3900
BUCKET:      <bucket-name>
YOUR ID:     <pod-name>
ACCESS KEY:  <access-key-id>
SECRET KEY:  <secret-access-key>
REGION:      garage

# First-time profile setup (run once):
aws configure set aws_access_key_id "<access-key-id>" --profile garagetytus
aws configure set aws_secret_access_key "<secret-access-key>" --profile garagetytus
aws configure set region garage --profile garagetytus

# PUT:
aws s3 cp ./your-file.md s3://<bucket-name>/from-<pod-name>/your-file.md \
    --endpoint http://10.42.42.1:3900 --profile garagetytus

# LIST:
aws s3 ls s3://<bucket-name>/ --recursive \
    --endpoint http://10.42.42.1:3900 --profile garagetytus

# GET:
aws s3 cp s3://<bucket-name>/from-mac/instructions.md ./ \
    --endpoint http://10.42.42.1:3900 --profile garagetytus

(naming + DO-NOT rules same as the boto3 block above)
```

Why two flavors: empirically, tytus pods (NemoClaw image) have
boto3 installed but NOT the aws CLI, AND `/etc/` is read-only
so we can't `aws configure` to drop credentials there. The
boto3 block gives the agent everything inline so no filesystem
writes are needed. The aws-CLI block exists for environments
where the operator has installed it deliberately.

## Multiple buckets — paste once per bucket

If the pod has access to several buckets (`work`, `personal`,
`agent-results`, …), paste this template once per bucket and
prefix each block with **"Bucket #1 — work:"**, **"Bucket #2 —
personal:"** etc. Inside each block, change `BUCKET` to the
right name. The agent will then know which bucket to use for
which kind of file.

For the multi-bucket case, append at the end:

```
ROUTING — which bucket for which task:
- work:          coding tasks, work-related files, project notes
- personal:      anything personal Sebastian asked you to handle
- agent-results: outputs you want OTHER pods (not Sebastian) to read

If a task doesn't clearly fit one of these, ASK SEBASTIAN.
```

## Filling the template programmatically

When v0.5.1 ships `garagetytus folder bind`, it'll emit this
prompt with everything already substituted:

```bash
garagetytus folder prompt --bucket work --pod wannolot-02 | pbcopy
```

For now, fill the four `<…>` fields manually after running:

```bash
ssh root@<droplet> garagetytus bucket grant <bucket> \
    --to <pod-name> --perms read,write --json
# → { "access_key_id": "GK…", "secret_access_key": "…" }
```
