import { getPlayerAccent } from "@/lib/player-colors";
import { getVisibleStackObjects } from "@/lib/stack-targets";

function registerName(map, id, name) {
  if (id == null) return;
  const key = String(id);
  if (!key) return;
  const text = String(name || "").trim();
  if (!text) return;
  map.set(key, text);
}

function registerController(map, id, controller) {
  if (id == null || controller == null) return;
  const key = String(id);
  if (!key) return;
  map.set(key, Number(controller));
}

export function buildObjectNameById(state) {
  const map = new Map();

  for (const player of state?.players || []) {
    for (const card of player?.hand_cards || []) {
      registerName(map, card?.id, card?.name);
    }
    for (const card of player?.graveyard_cards || []) {
      registerName(map, card?.id, card?.name);
    }
    for (const card of player?.exile_cards || []) {
      registerName(map, card?.id, card?.name);
    }
    for (const card of player?.command_cards || []) {
      registerName(map, card?.id, card?.name);
    }
    for (const card of player?.battlefield || []) {
      registerName(map, card?.id, card?.name);
      for (const memberId of card?.member_ids || []) {
        registerName(map, memberId, card?.name);
      }
    }
  }

  for (const stackObject of getVisibleStackObjects(state)) {
    registerName(map, stackObject?.id, stackObject?.name);
    registerName(map, stackObject?.inspect_object_id, stackObject?.name);
  }

  for (const card of state?.viewed_cards?.cards || []) {
    registerName(map, card?.id, card?.name);
  }

  return map;
}

export function buildObjectControllerById(state) {
  const map = new Map();

  for (const player of state?.players || []) {
    for (const zone of [
      player?.hand_cards || [],
      player?.graveyard_cards || [],
      player?.exile_cards || [],
      player?.command_cards || [],
    ]) {
      for (const card of zone) {
        registerController(map, card?.id, player?.id);
      }
    }

    for (const card of player?.battlefield || []) {
      registerController(map, card?.id, player?.id);
      for (const memberId of card?.member_ids || []) {
        registerController(map, memberId, player?.id);
      }
    }
  }

  for (const stackObject of getVisibleStackObjects(state)) {
    registerController(map, stackObject?.id, stackObject?.controller);
    registerController(map, stackObject?.inspect_object_id, stackObject?.controller);
  }

  const viewedSubject = state?.viewed_cards?.subject;
  for (const card of state?.viewed_cards?.cards || []) {
    registerController(map, card?.id, viewedSubject);
  }
  for (const cardId of state?.viewed_cards?.card_ids || []) {
    registerController(map, cardId, viewedSubject);
  }

  return map;
}

export function getObjectAccent(state, objectId, explicitControllerId = null) {
  if (objectId == null) return null;
  const controllerById = buildObjectControllerById(state);
  const controllerId = explicitControllerId != null
    ? Number(explicitControllerId)
    : controllerById.get(String(objectId));
  if (controllerId == null || Number(controllerId) === Number(state?.perspective)) {
    return null;
  }
  return getPlayerAccent(state?.players || [], controllerId);
}
