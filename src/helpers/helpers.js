export const groupSources = (sources) => {
  const bySourceId = new Map();

  for (const source of sources) {
    const existing = bySourceId.get(source.sourceId);
    if (existing) {
      existing.present = existing.present || source.present;
      if (!existing.folders.includes(source.folderPath)) {
        existing.folders.push(source.folderPath);
      }
    } else {
      bySourceId.set(source.sourceId, {
        sourceId: source.sourceId,
        browser: source.browser,
        profile: source.profile,
        present: source.present,
        folders: [source.folderPath],
      });
    }
  }

  const grouped = Array.from(bySourceId.values());

  const profilesPerBrowser = new Map();
  for (const source of grouped) {
    profilesPerBrowser.set(
      source.browser,
      (profilesPerBrowser.get(source.browser) ?? 0) + 1
    );
  }

  return grouped.map((source) => {
    const showProfile =
      source.profile !== 'Default' &&
      profilesPerBrowser.get(source.browser) > 1;

    return {
      sourceId: source.sourceId,
      label: showProfile
        ? `${source.browser} · ${source.profile}`
        : source.browser,
      present: source.present,
      folders: source.folders,
    };
  });
};

export const bookmarkStatus = (bookmark) => {
  const sources = bookmark.sources ?? [];
  if (sources.length === 0) {
    return 'manual';
  }
  return sources.some((source) => source.present) ? 'synced' : 'removed';
};

const SCHEMELESS_SCHEMES = /^(mailto|javascript|data|about):/i;

// Returns the link to save, or null when the input can't be a link.
export const normalizeLink = (raw) => {
  const value = raw.trim();
  if (!value) {
    return null;
  }

  const hasScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(value);
  const candidate =
    hasScheme || SCHEMELESS_SCHEMES.test(value) ? value : `https://${value}`;

  try {
    new URL(candidate);
    return candidate;
  } catch {
    return null;
  }
};

export const displayHost = (link) => {
  try {
    return new URL(link).hostname.replace(/^www\./, '') || link;
  } catch {
    return link;
  }
};

export const buildFolderTree = (folders) => {
  const nodes = new Map(
    folders.map((folder) => [folder.id, { ...folder, children: [] }])
  );
  const roots = [];

  for (const node of nodes.values()) {
    const parent = node.parentId == null ? null : nodes.get(node.parentId);
    (parent ? parent.children : roots).push(node);
  }

  const sortByPosition = (list) => {
    list.sort((a, b) => a.position - b.position || a.id - b.id);
    list.forEach((node) => sortByPosition(node.children));
  };
  sortByPosition(roots);

  return roots;
};

export const folderOptions = (folders) => {
  const options = [];
  const walk = (nodes, prefix) => {
    for (const node of nodes) {
      const path = prefix ? `${prefix} / ${node.name}` : node.name;
      options.push({ value: String(node.id), label: path });
      walk(node.children, path);
    }
  };
  walk(buildFolderTree(folders), '');
  return options;
};
