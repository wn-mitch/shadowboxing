Deployment Helper — Architecture Vision

The Problem with Rule Enforcement

The current phase system is a strict state machine. Drag is gated to
Movement/Charge, weapons are gated by range, targeting is gated by player. This has
already hit limits: pile-in moves, consolidation, fall-back, surge moves, reactive
moves, and charge moves all need to be encoded as separate cases. 40K 10th has
dozens of special movement abilities that each create exceptions.

The rules are also lookup-heavy (unit coherency, engagement range, desperate escape
tests) in ways this app has no data for.

The Better Model: Annotation Tools

Rather than enforcing what is allowed, give the player tools that document what
happened. Phase is context and display — it suggests the right tool and records
timeline snapshots — but it doesn't prevent actions. The player is the rules
arbiter; the app is a shared annotation layer.

This is how TTS works: a draw toolbar with a line tool, freehand, arrow, ruler,
etc. You use them to describe the game state. The software doesn't know 40K; the
players do.

Proposed Tool Palette

ActiveTool:
Select         — click units, show info, default mode
Move           — drag unit + draw movement arrow (replaces phase-gated drag)
Advance        — Move variant: marks unit as Advanced, orange arrow
FallBack       — Move variant: marks unit as Fell Back, red arrow
PileIn         — Move variant: 3" limit, purple arrow (Fight sub-move)
Consolidate    — Move variant: 3" limit, cyan arrow (Fight sub-move)
Measure        — click two points, shows edge-to-edge distance (ephemeral)
RangeRing      — click unit + enter radius, draws persistent ring from base edge
ShootAnnotate  — click shooter → weapon list → ring → click target → dashed line
Kill           — click unit → confirm → fades (works any time, either side)

How Phase Fits In

Phase still sequences the turn and records snapshots:

- Command: Command phase UI (placeholder today, CP spend + battle-shock later)
- Movement: Default tool = Move. Phase suggests Normal/Advance/Fall Back sub-types.
- Shooting: Default tool = ShootAnnotate. Weapon list in panel.
- Charge: Default tool = Move (charge move). 12" ring from base edge shown.
- Fight: Default tool = Move (pile-in), then Kill, then Move (consolidate).

"End Phase →" still records a snapshot. The timeline history still works
identically.

The key change: none of the tools are gated by phase. You can use the Kill tool in
Movement if a unit got sniped by a rule. You can use Move in Shooting if a unit has
a rule allowing it. You declare what type of move it is; the app annotates it
correctly.

Movement Annotation Detail

MovementArrow gains:
move_type: MoveType

MoveType:
Normal      → green arrow
Advance     → orange arrow (unit marked has_advanced)
FallBack    → red arrow   (unit marked has_fallen_back)
PileIn      → purple arrow (3" limit, Fight phase)
Consolidate → cyan arrow  (3" limit, Fight phase)
Charge      → orange arrow, dashed outline

The has_advanced / has_fallen_back flags on UnitBase are set by the tool, not
inferred from distance. The player explicitly declares what move they made.

Shoot Annotation Detail

No range enforcement — the annotation is informational. Player workflow:
1. Select ShootAnnotate tool
2. Click shooter unit → weapon list appears in panel
3. Select weapon → range ring appears (blue, from base edge)
4. Click target → dashed line drawn with distance label
5. Annotation is added to live-arrows and recorded in next snapshot

This handles every weapon exception without any app knowledge: rapid fire, heavy,
pistols in combat, etc. The player knows the rules; the app records the shot.

Measure Tool

Ephemeral ruler: click point A, click point B → floating annotation shows
edge-to-edge distance (nearest two bases, or if no units nearby, raw point
distance). Right-click to cancel. Not recorded.

This eliminates the need to encode any in-game ruler interactions. The player
physically measures; this supplements with precision.

Range Rings (General Tool)

Click any unit + set a radius → ring drawn from that unit's base edge. Used for:
- Aura abilities ("all units within 6" gain +1 to saves")
- Screening checks
- Deep strike exclusion zones

Multiple persistent rings can exist simultaneously. Each is tied to a unit and
moves when the unit moves. Dismissable via right-click or a clear button.

Fight Phase: Both Players Act

In 40K's Fight phase BOTH players' units pile in and fight. The sequencing
alternates (active player → opponent → active player → ...). The Kill tool should
therefore work on any unit from either side during Fight, not just "enemies."

The fight sequence naturally maps to:
1. Both players use PileIn move tool for their eligible units
2. Both players use Kill tool as they fight
3. Both players use Consolidate move tool after fighting

What This Doesn't Require Encoding

By moving to annotation-first:

- Engagement range enforcement ❌ (player knows if they're in combat)
- Unit coherency checking ❌ (player knows their coherency)
- Desperate Escape tests ❌ (player rolls, marks result)
- Fights First ordering ❌ (player knows their unit's abilities)
- Heavy weapon penalty tracking ❌ (player knows they moved)
- Wound allocation ❌ (player decides)
- Stratagem availability ❌ (player knows their CP)

What the app does well and should keep:
- LOS analysis (visibility polygon) ✅
- Range measurement (distance tool, range rings) ✅
- Timeline snapshots (recording what happened) ✅
- Movement annotation (arrows showing who moved where) ✅
- Deployment analysis (existing zone/coverage analysis) ✅

Implementation Phasing

Phase A (now): Bug fixes (active player, melee filter, range rings, base-edge
measurement, Fight kill fix)

Phase B: Tool palette skeleton — ActiveTool enum, CurrentTool resource, tool
selector in right panel, refactor handle_drag + handle_unit_click to dispatch on
tool

Phase C: Move sub-types — MoveType on MovementArrow, color variations,
Advance/FallBack/PileIn/Consolidate tools with distinct arrows

Phase D: ShootAnnotate tool replaces current weapon-gated click flow; dashed lines
added to snapshot

Phase E: Measure tool (ephemeral ruler); general RangeRing tool (click unit +
radius)
