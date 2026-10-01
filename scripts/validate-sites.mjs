import { readFile } from "node:fs/promises";

const data = JSON.parse(await readFile(new URL("../sites.json", import.meta.url), "utf8"));
const errors = [];
const names = new Set();

for (const [encoding, sites] of Object.entries(data)) {
  if (!["UTF-8", "EUC-KR"].includes(encoding) || !Array.isArray(sites)) {
    errors.push(`Invalid encoding group: ${encoding}`);
    continue;
  }

  for (const site of sites) {
    if (!site.name || names.has(site.name)) errors.push(`Missing or duplicate name: ${site.name ?? "(empty)"}`);
    if ((site.url?.match(/\{input\}/g) ?? []).length !== 1) {
      errors.push(`URL must contain exactly one {input} placeholder: ${site.name ?? "(unnamed)"}`);
    }
    try {
      const url = new URL(site.url.replace("{input}", "__FIGURE_SEARCH_INPUT__"));
      if (!["http:", "https:"].includes(url.protocol)) errors.push(`Unsupported protocol: ${site.name}`);
      const matches = [...url.searchParams.values()].filter((value) => value === "__FIGURE_SEARCH_INPUT__");
      if (matches.length !== 1) errors.push(`{input} must be a query value: ${site.name}`);
    } catch {
      errors.push(`Invalid URL: ${site.name ?? "(unnamed)"}`);
    }
    names.add(site.name);
  }
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}

console.log(`Validated ${names.size} configured stores.`);
