import { Search } from "lucide-react";
import type { MapCount, ModFilter, Perspective } from "../types";
import { formatNumber } from "../format";

interface Props {
  search: string;
  onSearch: (value: string) => void;
  maps: MapCount[];
  map: string;
  onMap: (value: string) => void;
  mods: ModFilter;
  onMods: (value: ModFilter) => void;
  perspective: Perspective;
  onPerspective: (value: Perspective) => void;
  hideEmpty: boolean;
  onHideEmpty: (value: boolean) => void;
  hideFull: boolean;
  onHideFull: (value: boolean) => void;
  hidePassword: boolean;
  onHidePassword: (value: boolean) => void;
}

export default function FilterBar(props: Props) {
  return (
    <div className="filters">
      <label className="search">
        <Search size={15} />
        <input
          type="text"
          placeholder="Search by name, IP or map…"
          value={props.search}
          onChange={(e) => props.onSearch(e.target.value)}
          spellCheck={false}
        />
      </label>

      <select value={props.map} onChange={(e) => props.onMap(e.target.value)}>
        <option value="">All maps</option>
        {props.maps.map((entry) => (
          <option key={entry.map} value={entry.map}>
            {entry.map} ({formatNumber(entry.servers)})
          </option>
        ))}
      </select>

      <select
        value={props.mods}
        onChange={(e) => props.onMods(e.target.value as ModFilter)}
      >
        <option value="all">Modded &amp; vanilla</option>
        <option value="modded">Modded only</option>
        <option value="vanilla">Vanilla only</option>
      </select>

      <select
        value={props.perspective}
        onChange={(e) => props.onPerspective(e.target.value as Perspective)}
      >
        <option value="all">Any perspective</option>
        <option value="fpp">1PP only</option>
        <option value="tpp">3PP allowed</option>
      </select>

      <label className="toggle">
        <input
          type="checkbox"
          checked={props.hideEmpty}
          onChange={(e) => props.onHideEmpty(e.target.checked)}
        />
        Hide empty
      </label>

      <label className="toggle">
        <input
          type="checkbox"
          checked={props.hideFull}
          onChange={(e) => props.onHideFull(e.target.checked)}
        />
        Hide full
      </label>

      <label className="toggle">
        <input
          type="checkbox"
          checked={props.hidePassword}
          onChange={(e) => props.onHidePassword(e.target.checked)}
        />
        Hide locked
      </label>
    </div>
  );
}
