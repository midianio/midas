import {
  Callout,
  Card,
  CardBody,
  CardHeader,
  Code,
  Divider,
  Grid,
  H1,
  H3,
  Pill,
  Row,
  Spacer,
  Stack,
  Stat,
  Table,
  Text,
  useCanvasState,
} from "cursor/canvas";

type Section =
  | "overview"
  | "commands"
  | "ownership"
  | "layers"
  | "build"
  | "caveats";

type OwnerFilter = "all" | "team" | "repo" | "midas";

const SECTIONS: { id: Section; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "commands", label: "Commands" },
  { id: "ownership", label: "Team vs repo" },
  { id: "layers", label: "How it mashes" },
  { id: "build", label: "What to build" },
  { id: "caveats", label: "Caveats" },
];

const COMMANDS: {
  area: string;
  cmd: string;
  role: string;
  agent: string;
}[] = [
  {
    area: "Daily",
    cmd: "dev",
    role: "Concurrent processes + optional pscale tunnel / migrate",
    agent: "shell",
  },
  {
    area: "Daily",
    cmd: "flow start|rebase|ship|tag|end|status|clean",
    role: "Branch / PR / release lifecycle",
    agent: "/midas-flow",
  },
  {
    area: "Daily",
    cmd: "migrate [apply|status]",
    role: "Forward-only local migrations",
    agent: "shell",
  },
  {
    area: "Gate",
    cmd: "check [--changed]",
    role: "Mechanical conformance (CI gate)",
    agent: "/midas-check",
  },
  {
    area: "Gate",
    cmd: "drift [SPEC]",
    role: "Read-only upgrade briefing",
    agent: "/midas-drift",
  },
  {
    area: "Standard",
    cmd: "sync [--check]",
    role: "Managed block in AGENTS.md",
    agent: "CI + doctor",
  },
  {
    area: "Standard",
    cmd: "explain <ID> · conventions",
    role: "Browse / explain embedded catalog",
    agent: "shell",
  },
  {
    area: "Standard",
    cmd: "deviate [ID] · --prune",
    role: "Ledger midas.toml [deviations]",
    agent: "via check flow",
  },
  {
    area: "Scaffold",
    cmd: "touch project|module|state|migration|component",
    role: "Golden generators — never hand-roll",
    agent: "/midas-touch",
  },
  {
    area: "Onboard",
    cmd: "adopt · doctor [--fix]",
    role: "Brownfield pin + env diagnosis",
    agent: "shell",
  },
];

const OWNERSHIP: {
  concern: string;
  owner: OwnerFilter;
  where: string;
  why: string;
}[] = [
  {
    concern: "Gate = midas check (or ledger) before PR",
    owner: "team",
    where: "Team Rule (enforce)",
    why: "True for every midas consumer",
  },
  {
    concern: "Scaffold = midas touch, never hand-roll",
    owner: "team",
    where: "Team Rule (enforce)",
    why: "Same anti-pattern everywhere",
  },
  {
    concern: "Pinned standard wins over stale local docs",
    owner: "team",
    where: "Team Rule",
    why: "Conflict policy is global",
  },
  {
    concern: "Agents: -y, prefer --json",
    owner: "team",
    where: "Team Rule",
    why: "CLI contract, not per-repo",
  },
  {
    concern: "Use named seams; don't reach around them",
    owner: "team",
    where: "Team Rule (short)",
    why: "Principle global; which seams are local",
  },
  {
    concern: "/midas-check · touch · flow · review",
    owner: "team",
    where: "Team Commands",
    why: "Same CLI verbs; no repo paths",
  },
  {
    concern: "Review contract (AGT-0006 JSON findings)",
    owner: "team",
    where: "Team Command + Bugbot Team Rules",
    why: "Vendor-neutral prompt is global",
  },
  {
    concern: "midas.toml pin, profile, stack, layout, deviations",
    owner: "repo",
    where: "Repo midas.toml",
    why: "Per project",
  },
  {
    concern: "Managed block in AGENTS.md",
    owner: "midas",
    where: "midas sync (repo files)",
    why: "Version-stamped, git-visible, CI-checkable",
  },
  {
    concern: "Domain skills with concrete paths (AGT-0008)",
    owner: "repo",
    where: ".claude/skills/…",
    why: "Local instance of each convention",
  },
  {
    concern: "Nested AGENTS.md, ARCHITECTURE, UL",
    owner: "repo",
    where: "Canon context files",
    why: "This tree's map",
  },
  {
    concern: "environment.json, terminals, install",
    owner: "repo",
    where: ".cursor/environment.json",
    why: "Different boot/deps per app",
  },
  {
    concern: "Project hooks / layout-specific .mdc globs",
    owner: "repo",
    where: ".cursor/hooks · rules",
    why: "Paths differ (app/api vs cli/)",
  },
  {
    concern: "GitNexus / impact analysis (AGT-0007)",
    owner: "repo",
    where: "Only if wired",
    why: "Not every repo has it",
  },
  {
    concern: "standards/ prose at a pin",
    owner: "midas",
    where: "Binary + [standard].version",
    why: "Not Cursor config — agent reads via check/explain",
  },
];

const BUILD_ITEMS: {
  id: string;
  title: string;
  scope: "team" | "repo" | "midas";
  priority: "now" | "later" | "skip";
  note: string;
}[] = [
  {
    id: "1",
    title: "Team Rule: midas always-on policy (≤15 lines)",
    scope: "team",
    priority: "now",
    note: "check / touch / seams / -y --json — do not paste sync block",
  },
  {
    id: "2",
    title: "Team Commands: /midas-check · touch · flow · review",
    scope: "team",
    priority: "now",
    note: "Thin wrappers that shell out to the CLI",
  },
  {
    id: "3",
    title: "Bugbot Team Rules: AGT-0006 review prompt",
    scope: "team",
    priority: "now",
    note: "Separate from Agent Team Rules dashboard",
  },
  {
    id: "4",
    title: "Keep midas sync managed block as-is",
    scope: "midas",
    priority: "now",
    note: "Pins version; CI midas sync --check; no Team duplicate",
  },
  {
    id: "5",
    title: "touch project / adopt: AGT-0008 domain skill stubs",
    scope: "midas",
    priority: "now",
    note: "Local paths filled per repo; Cursor discovers .claude/skills",
  },
  {
    id: "6",
    title: "Optional Cloud env template in touch project",
    scope: "midas",
    priority: "later",
    note: ".cursor/environment.json installs midas on PATH",
  },
  {
    id: "7",
    title: "Repo stop-hook nudge for midas check",
    scope: "repo",
    priority: "later",
    note: "Soft reminder — CI remains the hard gate",
  },
  {
    id: "8",
    title: "midas sync writing into .cursor/",
    scope: "midas",
    priority: "skip",
    note: "Fights don't-clobber-.cursor design",
  },
  {
    id: "9",
    title: "MCP server wrapping midas",
    scope: "midas",
    priority: "skip",
    note: "Shell + Team Commands enough unless typed allowlists needed",
  },
];

function ownerPill(owner: OwnerFilter) {
  if (owner === "team") return <Pill tone="info">Team</Pill>;
  if (owner === "repo") return <Pill tone="warning">Repo</Pill>;
  if (owner === "midas") return <Pill tone="success">Midas</Pill>;
  return <Pill tone="neutral">All</Pill>;
}

function priorityPill(p: "now" | "later" | "skip") {
  if (p === "now") return <Pill tone="success">Now</Pill>;
  if (p === "later") return <Pill tone="warning">Later</Pill>;
  return <Pill tone="neutral">Skip</Pill>;
}

function Overview() {
  return (
    <Stack gap={16}>
      <Callout tone="info" title="Principle">
        Cursor hosts when and how to invoke. Midas owns what is true and how to
        enforce it. Team owns non-repo-specific policy and /midas-* commands;
        repos keep pins, paths, and local domain skills.
      </Callout>

      <Grid columns={4} gap={12}>
        <Stat value="13" label="Top-level midas commands" tone="info" />
        <Stat value="Team" label="Always-on policy home" tone="success" />
        <Stat value="Repo" label="Pins · paths · domain skills" tone="warning" />
        <Stat value="CI" label="Hard gate = midas check" />
      </Grid>

      <Card>
        <CardHeader>Precedence (Agent chat)</CardHeader>
        <CardBody>
          <Text weight="semibold">Team Rules → Project Rules → User Rules</Text>
          <Text tone="secondary" size="small">
            AGENTS.md (incl. midas sync block) loads as project-layer
            context. Do not duplicate the managed block into Always Apply .mdc
            or Team Rules.
          </Text>
        </CardBody>
      </Card>

      <Card>
        <CardHeader>Target layout</CardHeader>
        <CardBody>
          <Code>{`Team dashboard
  Rules: midas policy (enforce)
  Commands: /midas-check · touch · flow · review
  Bugbot Team Rules: AGT-0006

Repo
  AGENTS.md              ← midas sync block
  midas.toml             ← pin, stack, deviations
  .claude/skills/*/      ← AGT-0008 local how-tos
  .cursor/environment.json, hooks  ← boot / soft nudges`}</Code>
        </CardBody>
      </Card>
    </Stack>
  );
}

function Commands() {
  return (
    <Stack gap={16}>
      <Text tone="secondary">
        Global flags on every command: <Code>--json</Code> ·{" "}
        <Code>--root</Code> · <Code>-y</Code> · <Code>-q</Code> ·{" "}
        <Code>-v</Code>. Agents should always pass <Code>-y</Code> and prefer{" "}
        <Code>--json</Code>.
      </Text>
      <Table
        headers={["Area", "Command", "Role", "Cursor surface"]}
        columnAlign={["left", "left", "left", "left"]}
        rows={COMMANDS.map((c) => [
          c.area,
          <Code>{c.cmd}</Code>,
          c.role,
          c.agent,
        ])}
        striped
        stickyHeader
      />
      <Callout tone="neutral" title="Planned, not shipped">
        <Code>setup</Code> / <Code>teardown</Code> · <Code>gen types</Code> ·{" "}
        <Code>upgrade</Code> · <Code>check --suggest</Code>
      </Callout>
    </Stack>
  );
}

function Ownership({ filter }: { filter: OwnerFilter }) {
  const rows = OWNERSHIP.filter(
    (r) => filter === "all" || r.owner === filter,
  );
  return (
    <Stack gap={16}>
      <Callout tone="success" title="Your instinct">
        Policy true for every midian repo → Team. Anything that names this
        repo's paths, pin, or stack → Repo. Midas still owns the sync block and
        the binary/catalog — not a second copy of the commandments in .cursor/.
      </Callout>
      <Table
        headers={["Concern", "Owner", "Where", "Why"]}
        rows={rows.map((r) => [
          r.concern,
          ownerPill(r.owner),
          r.where,
          r.why,
        ])}
        striped
        stickyHeader
        emptyMessage="No rows for this filter"
      />
    </Stack>
  );
}

function Layers() {
  return (
    <Stack gap={16}>
      <H3>Stack (top wins on conflict)</H3>
      <Card>
        <CardBody>
          <Stack gap={10}>
            <Row gap={8} align="center">
              <Pill tone="info">1 · Team</Pill>
              <Text>Always-on midas policy + /midas-* commands</Text>
            </Row>
            <Row gap={8} align="center">
              <Pill tone="success">2 · Midas sync</Pill>
              <Text>
                Version-stamped block in AGENTS.md (pin + pointers)
              </Text>
            </Row>
            <Row gap={8} align="center">
              <Pill tone="warning">3 · Repo</Pill>
              <Text>
                Domain skills, nested AGENTS, environment.json, path globs
              </Text>
            </Row>
            <Row gap={8} align="center">
              <Pill tone="neutral">4 · Mechanical</Pill>
              <Text>
                <Code>midas check</Code> in CI — only hard gate
              </Text>
            </Row>
            <Row gap={8} align="center">
              <Pill tone="neutral">5 · Semantic</Pill>
              <Text>
                External reviewer + <Code>review-agent-prompt.md</Code> /
                Bugbot
              </Text>
            </Row>
          </Stack>
        </CardBody>
      </Card>

      <H3>Do / don't</H3>
      <Grid columns={2} gap={12}>
        <Card>
          <CardHeader trailing={<Pill tone="success">Do</Pill>}>
            Team
          </CardHeader>
          <CardBody>
            <Stack gap={6}>
              <Text size="small">Short enforced policy</Text>
              <Text size="small">/midas-* command wrappers</Text>
              <Text size="small">Bugbot Team Rules for AGT-0006</Text>
            </Stack>
          </CardBody>
        </Card>
        <Card>
          <CardHeader trailing={<Pill tone="warning">Don't</Pill>}>
            Duplicate
          </CardHeader>
          <CardBody>
            <Stack gap={6}>
              <Text size="small">Paste sync block into Team Rules</Text>
              <Text size="small">Put AGT-0008 path details at Team</Text>
              <Text size="small">Have midas sync clobber .cursor/</Text>
            </Stack>
          </CardBody>
        </Card>
      </Grid>
    </Stack>
  );
}

function Build() {
  const now = BUILD_ITEMS.filter((b) => b.priority === "now");
  const later = BUILD_ITEMS.filter((b) => b.priority === "later");
  const skip = BUILD_ITEMS.filter((b) => b.priority === "skip");
  return (
    <Stack gap={16}>
      <Callout tone="info" title="Recommended first slice">
        Dashboard: Team Rule + Team Commands + Bugbot rules. In midas repo:
        keep sync; add AGT-0008 skill stubs on touch/adopt. Skip sync-of-.cursor
        and MCP for now.
      </Callout>

      <H3>Now</H3>
      <Table
        headers={["#", "Item", "Scope", "Note"]}
        rows={now.map((b) => [
          b.id,
          b.title,
          ownerPill(b.scope),
          b.note,
        ])}
        striped
      />

      <H3>Later</H3>
      <Table
        headers={["#", "Item", "Scope", "Note"]}
        rows={later.map((b) => [
          b.id,
          b.title,
          ownerPill(b.scope),
          b.note,
        ])}
        striped
      />

      <H3>Skip (for now)</H3>
      <Table
        headers={["#", "Item", "Scope", "Note"]}
        rows={skip.map((b) => [
          b.id,
          b.title,
          ownerPill(b.scope),
          b.note,
        ])}
        striped
      />
    </Stack>
  );
}

function Caveats() {
  return (
    <Stack gap={12}>
      <Card collapsible defaultOpen>
        <CardHeader>Team Rules hit every repo on the Cursor team</CardHeader>
        <CardBody>
          <Text>
            If the team also has non-midas repos, prefer optional Team Rules or
            a glob like <Code>**/midas.toml</Code> (applies when that file is in
            context — not a perfect "repo has midas" detector). Enforced midas
            rules on unrelated repos will confuse agents.
          </Text>
        </CardBody>
      </Card>
      <Card collapsible defaultOpen>
        <CardHeader>Team Commands ≠ Team Skills</CardHeader>
        <CardBody>
          <Text>
            Commands are the right home for CLI wrappers. Skills as dashboard
            free-form content aren't first-class the same way — distribute via
            Team Marketplace plugin if you want one installable pack, or keep
            procedural bits as Team Commands and local domain skills in-repo.
          </Text>
        </CardBody>
      </Card>
      <Card collapsible defaultOpen>
        <CardHeader>Bugbot is a separate rules surface</CardHeader>
        <CardBody>
          <Text>
            Agent Team Rules from the Rules/Commands/Hooks dashboard do not
            automatically apply to Bugbot. Put the reviewer contract in Bugbot
            Team Rules (and optionally <Code>.cursor/BUGBOT.md</Code> for repo
            nuance).
          </Text>
        </CardBody>
      </Card>
      <Card collapsible defaultOpen>
        <CardHeader>Cloud Agents + hooks</CardHeader>
        <CardBody>
          <Text>
            Cloud Agents get Team Rules/Commands; project hooks always; Team
            Hooks are Enterprise-only. Soft "run check before stop" may stay as
            a repo hook or Team Command reminder until you have team hooks.
          </Text>
        </CardBody>
      </Card>
      <Divider />
      <Text tone="tertiary" size="small">
        Sources: standards/agents.md · standards/review-agent-prompt.md · SPEC.md
        §5/§8 · Cursor Rules / Skills / Cloud Agent best practices docs.
      </Text>
    </Stack>
  );
}

export default function MidasCursorIntegration() {
  const [section, setSection] = useCanvasState<Section>("section", "overview");
  const [ownerFilter, setOwnerFilter] = useCanvasState<OwnerFilter>(
    "ownerFilter",
    "all",
  );

  return (
    <Stack gap={20} style={{ padding: 20, maxWidth: 1100 }}>
      <Stack gap={6}>
        <H1>Midas × Cursor</H1>
        <Text tone="secondary">
          Review surface for command map, team vs repo ownership, and what to
          implement next.
        </Text>
      </Stack>

      <Row gap={8} wrap>
        {SECTIONS.map((s) => (
          <Pill
            key={s.id}
            active={section === s.id}
            onClick={() => setSection(s.id)}
          >
            {s.label}
          </Pill>
        ))}
        <Spacer />
        {priorityPill("now")}
        <Text size="small" tone="tertiary">
          midas 0.5.0
        </Text>
      </Row>

      <Divider />

      {section === "ownership" ? (
        <Row gap={8} wrap align="center">
          <Text size="small" tone="secondary">
            Filter owner:
          </Text>
          {(
            [
              ["all", "All"],
              ["team", "Team"],
              ["repo", "Repo"],
              ["midas", "Midas"],
            ] as const
          ).map(([id, label]) => (
            <Pill
              key={id}
              size="sm"
              active={ownerFilter === id}
              onClick={() => setOwnerFilter(id)}
            >
              {label}
            </Pill>
          ))}
        </Row>
      ) : null}

      {section === "overview" ? <Overview /> : null}
      {section === "commands" ? <Commands /> : null}
      {section === "ownership" ? <Ownership filter={ownerFilter} /> : null}
      {section === "layers" ? <Layers /> : null}
      {section === "build" ? <Build /> : null}
      {section === "caveats" ? <Caveats /> : null}
    </Stack>
  );
}
