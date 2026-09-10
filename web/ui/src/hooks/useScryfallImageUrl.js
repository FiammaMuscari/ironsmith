import { useEffect, useMemo, useState } from "react";
import {
  resolveScryfallImageUrl,
  scryfallImageUrl,
} from "@/lib/scryfall";

export function useScryfallImage(cardName, version = "normal") {
  const query = String(cardName || "").trim();
  const imageVersion = String(version || "normal").trim() || "normal";
  // A locale is a text concern, not a printing concern. Scryfall's localized
  // lookup returns an arbitrary printing with the same oracle id, which can
  // have different art, set, frame and collector number. Keep the image tied
  // to the selected English printing while the text layer localizes separately.
  const key = useMemo(() => `${query}|${imageVersion}`, [imageVersion, query]);
  const cached = scryfallImageUrl(query, imageVersion);
  const [resolved, setResolved] = useState(() => ({
    key,
    url: cached,
    settled: Boolean(cached) || !query,
  }));
  const currentUrl = (resolved.key === key && resolved.url) ? resolved.url : cached;

  useEffect(() => {
    let cancelled = false;
    if (cached || !query) return undefined;

    resolveScryfallImageUrl(query, imageVersion)
      .then((url) => {
        if (!cancelled) {
          setResolved({ key, url: url || "", settled: true });
        }
      })
      .catch(() => {
        if (!cancelled) {
          setResolved({ key, url: "", settled: true });
        }
      });

    return () => {
      cancelled = true;
    };
  }, [cached, imageVersion, key, query]);

  return {url: currentUrl, ready: Boolean(currentUrl) || !query || (resolved.key === key && resolved.settled)};
}

export default function useScryfallImageUrl(cardName, version = "normal") {
  return useScryfallImage(cardName, version).url;
}
