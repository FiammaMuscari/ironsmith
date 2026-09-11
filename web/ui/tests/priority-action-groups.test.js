import test from "node:test";
import assert from "node:assert/strict";
import {
  buildBattlefieldFamilies,
  buildPriorityActionGroups,
  formatCastActionGroupLabel,
  isMultiCastActionGroup,
} from "../src/lib/priority-action-groups.js";

function castAction({
  index,
  objectId = 101,
  label,
  fromZone = "hand",
  castingMethod,
}) {
  return {
    index,
    label,
    kind: "cast_spell",
    object_id: objectId,
    from_zone: fromZone,
    action_ref: {
      kind: "cast_spell",
      spell_id: objectId,
      from_zone: fromZone,
      casting_method: castingMethod,
    },
  };
}

test("cast action labels hide method qualifiers at the group level", () => {
  assert.equal(
    formatCastActionGroupLabel("Cast Force of Will (alternative #0)"),
    "Cast Force of Will"
  );
  assert.equal(
    formatCastActionGroupLabel("Cast Gravecrawler (from graveyard)"),
    "Cast Gravecrawler"
  );
});

test("normal and alternative casts for one card collapse behind one cast group", () => {
  const groups = buildPriorityActionGroups([
    castAction({
      index: 3,
      label: "Cast Force of Will",
      castingMethod: { kind: "normal" },
    }),
    castAction({
      index: 4,
      label: "Cast Force of Will (alternative #0)",
      castingMethod: { kind: "alternative", index: 0 },
    }),
  ], buildBattlefieldFamilies([]));

  assert.equal(groups.length, 1);
  assert.equal(groups[0].label, "Cast Force of Will");
  assert.equal(groups[0].count, 1);
  assert.equal(groups[0].firstAction.index, 3);
  assert.deepEqual([...groups[0].actionIndices], [3, 4]);
});

test("a graveyard normal-cost cast permission stays as one cast group", () => {
  const groups = buildPriorityActionGroups([
    castAction({
      index: 8,
      objectId: 202,
      label: "Cast Gravecrawler (from graveyard)",
      fromZone: "graveyard",
      castingMethod: {
        kind: "play_from",
        source: 202,
        zone: "graveyard",
        use_alternative: null,
      },
    }),
  ], buildBattlefieldFamilies([]));

  assert.equal(groups.length, 1);
  assert.equal(groups[0].label, "Cast Gravecrawler");
  assert.equal(groups[0].firstAction.index, 8);
});

test("alternative-only casts still expose a generic cast group", () => {
  const groups = buildPriorityActionGroups([
    castAction({
      index: 11,
      label: "Cast Force of Will (alternative #0)",
      castingMethod: { kind: "alternative", index: 0 },
    }),
  ], buildBattlefieldFamilies([]));

  assert.equal(groups.length, 1);
  assert.equal(groups[0].label, "Cast Force of Will");
  assert.equal(groups[0].firstAction.index, 11);
});

test("multi-method cast groups suppress card previews while choosing a method", () => {
  const [group] = buildPriorityActionGroups([
    castAction({ index: 1, label: "Cast Fireball", castingMethod: { kind: "normal" } }),
    castAction({ index: 2, label: "Cast Fireball (alternative #0)", castingMethod: { kind: "alternative", index: 0 } }),
  ], buildBattlefieldFamilies([]));

  assert.equal(isMultiCastActionGroup(group), true);
  assert.equal(isMultiCastActionGroup({ actions: [{ kind: "cast_spell" }] }), false);
  assert.equal(isMultiCastActionGroup({ actions: [{ kind: "activate_ability" }, { kind: "activate_ability" }] }), false);
});
