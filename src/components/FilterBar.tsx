// SPDX-License-Identifier: GPL-3.0-or-later
import { Search } from "lucide-react";
import type { MapCount, ModFilter, Perspective } from "../types";
import { formatNumber } from "../format";
import Select from "./Select";

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

const MOD_OPTIONS = [
  { value: "all", label: "Modded & vanilla" },
  { value: "modded", label: "Modded only" },
  { value: "vanilla", label: "Vanilla only" },
];

const PERSPECTIVE_OPTIONS = [
  { value: "all", label: "Any perspective" },
  { value: "fpp", label: "1PP only" },
  { value: "tpp", label: "3PP allowed" },
];

export default function FilterBar(props: Props) {
  const mapOptions = [
    { value: "", label: "All maps" },
    ...props.maps.map((entry) => ({
      value: entry.map,
      label: entry.map,
      hint: formatNumber(entry.servers),
    })),
  ];

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

      <Select
        label="Map"
        value={props.map}
        options={mapOptions}
        onChange={props.onMap}
        width={190}
      />

      <Select
        label="Mods"
        value={props.mods}
        options={MOD_OPTIONS}
        onChange={(v) => props.onMods(v as ModFilter)}
        width={168}
      />

      <Select
        label="Perspective"
        value={props.perspective}
        options={PERSPECTIVE_OPTIONS}
        onChange={(v) => props.onPerspective(v as Perspective)}
        width={158}
      />

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
