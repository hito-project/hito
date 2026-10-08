#!/usr/bin/env bash
# Migrates the staged docs (backlog, research, open questions) to GitHub.
#
# Usage:
#   scripts/migrate-to-github.sh --repo OWNER/NAME            # dry run: parse and print the plan, no network
#   scripts/migrate-to-github.sh --repo OWNER/NAME --apply    # create labels, milestones, issues, discussions
#
# Requirements for --apply:
#   - gh authenticated with access to the repo (gh auth login)
#   - Discussions enabled on the repo, with a custom category named "Research"
#     (Settings -> Discussions; categories can't be created through the API)
#
# The script is idempotent: issues, milestones and discussions whose titles already exist are skipped.
# Backlog file format: docs/backlog/README.md
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKLOG="$ROOT/docs/backlog"
REPO=""
APPLY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo) REPO="$2"; shift 2 ;;
    --apply) APPLY=1; shift ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done
[[ -n "$REPO" ]] || { echo "Missing --repo OWNER/NAME" >&2; exit 2; }
BASE_URL="https://github.com/$REPO/blob/main"

# name|color|description
LABELS=(
  "type:epic|5319e7|A capability area delivered by several stories"
  "type:story|0e8a16|A user-visible increment, sub-issue of an epic"
  "type:spike|fbca04|Time-boxed research ending in an ADR or design doc"
  "type:bug|d73a4a|Something doesn't work as expected"
  "area:research|c5def5|Research and technical decisions"
  "area:infra|c5def5|Build, CI, packaging, repository"
  "area:core|c5def5|Element store, schemas, transactions"
  "area:ui|c5def5|Application shell, panels, commands"
  "area:viewport|c5def5|3D viewport and rendering"
  "area:geometry|c5def5|Geometry kernel and element geometry"
  "area:structural|c5def5|Structural domain"
  "area:architecture|c5def5|Architectural domain"
  "area:mep|c5def5|MEP domain"
  "area:civil|c5def5|Civil, infrastructure, geotechnical and water"
  "area:drafting|c5def5|2D drafting"
  "area:drawings|c5def5|Views, annotation, sheets, exports"
  "area:file-format|c5def5|Native file format"
  "area:interop|c5def5|IFC, DWG, RVT and other exchange formats"
  "area:i18n|c5def5|Localization, units, regional packs"
)

# file (relative to docs/)|discussion category
DISCUSSIONS=(
  "research/revit-capabilities.md|Research"
  "research/civil-engineer-workflows.md|Research"
  "research/aec-software-landscape.md|Research"
  "research/why-separate-products.md|Research"
  "research/industry-pain-points.md|Research"
  "research/prior-art.md|Research"
  "open-questions/suite-definition.md|Ideas"
  "open-questions/founding-user-questions.md|Q&A"
)

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

log() { printf '%s\n' "$*"; }
dry() { [[ $APPLY -eq 0 ]]; }

# Rewrites links in a body so they resolve on GitHub.
#   $1 = directory of the source file relative to the repo root ("" for backlog bodies, whose links are repo-root relative)
absolutize_links() {
  local dir="$1"
  if [[ -z "$dir" ]]; then
    sed -E "s#\]\((docs/[^)]*)\)#](${BASE_URL}/\1)#g"
  else
    local parent="${dir%/*}"
    sed -E \
      -e "s#\]\(\.\./([^)]*)\)#](${BASE_URL}/${parent}/\1)#g" \
      -e "s#\]\(([a-z0-9-]+\.md[^)]*)\)#](${BASE_URL}/${dir}/\1)#g"
  fi
}

strip_quotes() { sed -E 's/^"(.*)"$/\1/'; }

meta() { # $1=meta file, $2=key
  sed -n "s/^$2:[[:space:]]*//p" "$1" | strip_quotes
}

# Splits an epic file into $out/meta, $out/epic.body, $out/storyN.{title,labels,body}
parse_epic() {
  local file="$1" out="$2"
  mkdir -p "$out"
  : > "$out/meta"; : > "$out/epic.body"
  awk -v out="$out" -v file="$file" '
    NR == 1 && /^---$/ { fm = 1; next }
    fm == 1 && /^---$/ { fm = 2; next }
    fm == 1 { print > (out "/meta"); next }
    /^## Story: / {
      n++; t = $0; sub(/^## Story: /, "", t)
      print t > (out "/story" n ".title"); printf "" > (out "/story" n ".body")
      want_labels = 1; next
    }
    want_labels {
      if ($0 !~ /^labels:/) { printf "%s: story %d has no labels: line\n", file, n > "/dev/stderr"; exit 1 }
      l = $0; sub(/^labels:[ ]*/, "", l); print l > (out "/story" n ".labels")
      want_labels = 0; next
    }
    n == 0 { print > (out "/epic.body"); next }
    { print > (out "/story" n ".body") }
    END { if (fm != 2) { printf "%s: missing front matter\n", file > "/dev/stderr"; exit 1 } }
  ' "$file"
}

known_label() {
  local l
  for l in "${LABELS[@]}"; do [[ "${l%%|*}" == "$1" ]] && return 0; done
  return 1
}

check_labels() { # $1 = comma-separated labels, $2 = context
  local l
  IFS=',' read -ra parts <<< "$1"
  for l in "${parts[@]}"; do
    l="$(echo "$l" | xargs)"
    known_label "$l" || { echo "$2: unknown label '$l' (add it to LABELS)" >&2; exit 1; }
  done
}

# ---------- GitHub helpers (apply mode only) ----------

existing_issues() { # number<TAB>title for every issue
  gh issue list -R "$REPO" --state all --limit 2000 --json number,title --jq '.[] | "\(.number)\t\(.title)"'
}

find_issue() { # $1 = title -> number or empty
  awk -F'\t' -v t="$1" '$2 == t { print $1; exit }' "$WORK/issues.tsv"
}

create_issue() { # $1 title, $2 body file, $3 labels, $4 milestone -> prints number
  local args=(-R "$REPO" --title "$1" --body-file "$2" --label "$(echo "$3" | sed 's/, */,/g')")
  [[ -n "$4" ]] && args+=(--milestone "$4")
  local url
  url="$(gh issue create "${args[@]}")"
  local num="${url##*/}"
  printf '%s\t%s\n' "$num" "$1" >> "$WORK/issues.tsv"
  echo "$num"
}

add_sub_issue() { # $1 parent number, $2 child number
  local child_id
  child_id="$(gh api "repos/$REPO/issues/$2" --jq .id)"
  gh api -X POST "repos/$REPO/issues/$1/sub_issues" -F sub_issue_id="$child_id" > /dev/null 2>&1 \
    || log "    (sub-issue link #$2 -> #$1 skipped: already linked or not permitted)"
}

# ---------- 1. Labels ----------

log "== Labels"
for entry in "${LABELS[@]}"; do
  IFS='|' read -r name color desc <<< "$entry"
  if dry; then log "  label  $name"; else
    gh label create "$name" -R "$REPO" --color "$color" --description "$desc" --force > /dev/null
    log "  label  $name"
  fi
done

# ---------- 2. Milestones ----------

log "== Milestones"
if ! dry; then gh api "repos/$REPO/milestones?state=all&per_page=100" --jq '.[].title' > "$WORK/milestones.txt"; fi
for readme in "$BACKLOG"/m*/README.md; do
  title="$(sed -n 's/^# //p' "$readme" | head -1)"
  desc="$(sed -n 's/^\*\*Done when:\*\* //p' "$readme" | head -1)"
  if dry; then log "  milestone  $title"; continue; fi
  if grep -qxF "$title" "$WORK/milestones.txt"; then log "  milestone  $title (exists)"; continue; fi
  gh api "repos/$REPO/milestones" -f title="$title" -f description="Done when: $desc" > /dev/null
  log "  milestone  $title"
done

# ---------- 3. Epics and stories ----------

log "== Epics and stories"
if ! dry; then existing_issues > "$WORK/issues.tsv"; fi
total_epics=0; total_stories=0
for file in "$BACKLOG"/m*/e*.md "$BACKLOG"/future/e*.md; do
  rel="${file#"$ROOT"/}"
  out="$WORK/$(basename "$file" .md)"
  parse_epic "$file" "$out"
  title="$(meta "$out/meta" title)"; labels="$(meta "$out/meta" labels)"; milestone="$(meta "$out/meta" milestone)"
  [[ -n "$title" && -n "$labels" ]] || { echo "$rel: front matter needs title and labels" >&2; exit 1; }
  check_labels "$labels" "$rel"
  { absolutize_links "" < "$out/epic.body"; printf '\n---\n_Migrated from `%s`._\n' "$rel"; } > "$out/epic.final"
  total_epics=$((total_epics + 1))

  if dry; then
    ms_note=""; [[ -n "$milestone" ]] && ms_note="  {$milestone}"
    log "  EPIC  $title  [$labels]$ms_note"
  else
    epic_num="$(find_issue "$title")"
    if [[ -n "$epic_num" ]]; then log "  EPIC  #$epic_num $title (exists)"
    else epic_num="$(create_issue "$title" "$out/epic.final" "$labels" "$milestone")"; log "  EPIC  #$epic_num $title"; fi
  fi

  for tfile in "$out"/story*.title; do
    [[ -e "$tfile" ]] || continue
    n="$(basename "$tfile" .title)"
    s_title="$(cat "$tfile")"; s_labels="$(cat "$out/$n.labels")"
    check_labels "$s_labels" "$rel ($s_title)"
    { absolutize_links "" < "$out/$n.body"; printf '\n---\n_Part of epic: %s. Migrated from `%s`._\n' "$title" "$rel"; } > "$out/$n.final"
    total_stories=$((total_stories + 1))
    if dry; then log "      story  $s_title  [$s_labels]"; continue; fi
    s_num="$(find_issue "$s_title")"
    if [[ -n "$s_num" ]]; then log "      story  #$s_num $s_title (exists)"
    else s_num="$(create_issue "$s_title" "$out/$n.final" "$s_labels" "$milestone")"; log "      story  #$s_num $s_title"; fi
    add_sub_issue "$epic_num" "$s_num"
  done
done
log "  ($total_epics epics, $total_stories stories)"

# ---------- 4. Discussions ----------

log "== Discussions"
if ! dry; then
  owner="${REPO%%/*}"; name="${REPO##*/}"
  repo_id="$(gh api graphql -f owner="$owner" -f name="$name" -f query='query($owner:String!,$name:String!){repository(owner:$owner,name:$name){id}}' --jq .data.repository.id)"
  gh api graphql -f owner="$owner" -f name="$name" -f query='query($owner:String!,$name:String!){repository(owner:$owner,name:$name){discussionCategories(first:50){nodes{id name}}}}' \
    --jq '.data.repository.discussionCategories.nodes[] | "\(.name)\t\(.id)"' > "$WORK/categories.tsv"
  gh api graphql -f owner="$owner" -f name="$name" -f query='query($owner:String!,$name:String!){repository(owner:$owner,name:$name){discussions(first:100){nodes{title}}}}' \
    --jq '.data.repository.discussions.nodes[].title' > "$WORK/discussions.txt"
fi
for entry in "${DISCUSSIONS[@]}"; do
  IFS='|' read -r rel category <<< "$entry"
  src="$ROOT/docs/$rel"
  [[ -f "$src" ]] || { echo "Missing $src" >&2; exit 1; }
  d_title="$(sed -n 's/^# //p' "$src" | head -1)"
  if dry; then log "  discussion  [$category] $d_title"; continue; fi
  if grep -qxF "$d_title" "$WORK/discussions.txt"; then log "  discussion  [$category] $d_title (exists)"; continue; fi
  cat_id="$(awk -F'\t' -v c="$category" '$1 == c { print $2; exit }' "$WORK/categories.tsv")"
  [[ -n "$cat_id" ]] || { echo "Discussion category '$category' not found. Create it in the repo settings." >&2; exit 1; }
  body="$(sed '1{/^# /d}' "$src" | absolutize_links "docs/$(dirname "$rel")"; printf '\n---\n_Migrated from `docs/%s`._\n' "$rel")"
  gh api graphql -f repo="$repo_id" -f cat="$cat_id" -f title="$d_title" -f body="$body" -f query='
    mutation($repo: ID!, $cat: ID!, $title: String!, $body: String!) {
      createDiscussion(input: {repositoryId: $repo, categoryId: $cat, title: $title, body: $body}) { discussion { url } }
    }' --jq .data.createDiscussion.discussion.url | sed "s/^/  discussion  [$category] /"
done

if dry; then log ""; log "Dry run only. Nothing was sent to GitHub. Re-run with --apply to migrate."; fi
