import React, { useState, useEffect } from "react";
import { 
  Server, 
  Flame, 
  Star, 
  History, 
  FolderCheck, 
  Settings as SettingsIcon, 
  Search, 
  Play, 
  RefreshCw, 
  Terminal, 
  ShieldAlert, 
  HardDrive, 
  ExternalLink,
  Users,
  Wifi,
  Lock,
  Compass,
  CheckCircle2,
  AlertCircle
} from "lucide-react";
import { ServerItem, SteamEnvironment, InstalledModInfo, AppUserConfig, DzsaQueryResult } from "./types";

// Helper to call desktop IPC (Electron or Tauri)
async function callDesktopIpc<T>(command: string, args?: any): Promise<T> {
  try {
    if (typeof window !== "undefined" && (window as any).require) {
      const { ipcRenderer } = (window as any).require("electron");
      return await ipcRenderer.invoke(command, args);
    }
  } catch (_) {}

  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<T>(command, args);
  } catch (_) {}

  // Fallback demo mock
  console.log(`[Web Demo Fallback] Command '${command}' invoked with:`, args);
  if (command === "get_steam_environment") {
    return {
      is_flatpak: false,
      steam_root: "/home/user/.local/share/Steam",
      dayz_dir: "/home/user/.local/share/Steam/steamapps/common/DayZ",
      workshop_dir: "/home/user/.local/share/Steam/steamapps/workshop/content/221100",
      dayz_found: true
    } as unknown as T;
  }
  return [] as unknown as T;
}

const INITIAL_SERVERS: ServerItem[] = [
  { id: "1", name: "[US] DayZ Underground - Hardcore Survival (1PP)", ip: "192.223.24.12", port: 2302, query_port: 27016, map: "Chernarus", players: 54, max_players: 60, ping: 32, is_modded: true, mods_count: 14, is_3pp: false, has_password: false },
  { id: "2", name: "[EU] ZERO Namalsk | Hardcore 1PP", ip: "176.57.171.215", port: 2302, query_port: 27016, map: "Namalsk", players: 40, max_players: 40, ping: 48, is_modded: true, mods_count: 8, is_3pp: false, has_password: false },
  { id: "3", name: "[EU] Spaggie Chernarus Vanilla (No Mods)", ip: "109.230.208.77", port: 2302, query_port: 27016, map: "Chernarus", players: 58, max_players: 60, ping: 24, is_modded: false, mods_count: 0, is_3pp: true, has_password: false },
  { id: "4", name: "[US] Rearmed Deer Isle | 100k Start | Traders | Codelock", ip: "147.135.30.12", port: 2302, query_port: 27016, map: "DeerIsle", players: 88, max_players: 100, ping: 85, is_modded: true, mods_count: 32, is_3pp: true, has_password: false },
  { id: "5", name: "[SAKHAL] Frostline Official Server EU 1-10", ip: "185.242.115.10", port: 2302, query_port: 27016, map: "Sakhal", players: 60, max_players: 60, ping: 19, is_modded: false, mods_count: 0, is_3pp: true, has_password: false },
  { id: "6", name: "[EU] Sunnyvale Sakhal #1 | Helicopters | BTR | Base Building", ip: "149.202.89.44", port: 2302, query_port: 27016, map: "Sakhal", players: 74, max_players: 80, ping: 52, is_modded: true, mods_count: 22, is_3pp: true, has_password: false }
];

export default function App() {
  const [activeTab, setActiveTab] = useState<"servers" | "favorites" | "history" | "mods" | "settings">("servers");
  const [steamEnv, setSteamEnv] = useState<SteamEnvironment | null>(null);
  const [config, setConfig] = useState<AppUserConfig>({
    player_name: "Survivor",
    is_flatpak: false,
    use_gamemode: false,
    use_mangohud: false,
    custom_launch_params: "-nolauncher -world=empty",
    favorites: []
  });
  const [servers, setServers] = useState<ServerItem[]>(INITIAL_SERVERS);
  const [installedMods, setInstalledMods] = useState<InstalledModInfo[]>([]);
  const [selectedServer, setSelectedServer] = useState<ServerItem | null>(null);
  const [serverModalMods, setServerModalMods] = useState<{ workshop_id: string; name: string }[]>([]);
  const [loadingDzsa, setLoadingDzsa] = useState<boolean>(false);
  const [launchStatus, setLaunchStatus] = useState<string | null>(null);

  // Filters
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedMap, setSelectedMap] = useState("ALL");
  const [maxPing, setMaxPing] = useState(300);
  const [modFilter, setModFilter] = useState<"ALL" | "MODDED" | "VANILLA">("ALL");

  useEffect(() => {
    callDesktopIpc<SteamEnvironment>("get_steam_environment")
      .then((env) => env && setSteamEnv(env))
      .catch(console.error);

    callDesktopIpc<InstalledModInfo[]>("get_installed_workshop_mods")
      .then((mods) => Array.isArray(mods) && setInstalledMods(mods))
      .catch(console.error);
  }, []);

  const handleSaveConfig = (newCfg: AppUserConfig) => {
    setConfig(newCfg);
    callDesktopIpc("save_app_config", { config: newCfg }).catch(() => {});
  };

  const toggleFavorite = (serverId: string) => {
    const isFav = config.favorites.includes(serverId);
    const updatedFavs = isFav
      ? config.favorites.filter((id) => id !== serverId)
      : [...config.favorites, serverId];
    
    handleSaveConfig({ ...config, favorites: updatedFavs });
  };

  const handleSelectServer = async (srv: ServerItem) => {
    setSelectedServer(srv);
    setLoadingDzsa(true);
    try {
      const res = await callDesktopIpc<any>("query_dzsa", { ip: srv.ip, queryPort: srv.query_port });
      if (res && res.result && res.result.mods) {
        setServerModalMods(res.result.mods.map((m: any) => ({ workshop_id: m.steamWorkshopId, name: m.name })));
      } else {
        setServerModalMods([]);
      }
    } catch (e) {
      setServerModalMods([]);
    } finally {
      setLoadingDzsa(false);
    }
  };

  const handleLaunchServer = async (srv: ServerItem) => {
    setLaunchStatus(`Preparando simlinks y lanzando DayZ para ${srv.name}...`);
    try {
      const modIds = serverModalMods.map((m) => m.workshop_id);
      const res = await callDesktopIpc<string>("launch_game", {
        ipPort: `${srv.ip}:${srv.port}`,
        modIds: modIds,
        customName: config.player_name,
        useGamemode: config.use_gamemode,
        useMangohud: config.use_mangohud,
        customArgs: config.custom_launch_params
      });
      setLaunchStatus(res || "¡DayZ ejecutado con éxito!");
      setTimeout(() => setLaunchStatus(null), 8000);
    } catch (err: any) {
      setLaunchStatus(`Error: ${err.message || err}`);
    }
  };

  const filteredServers = servers.filter((srv) => {
    if (activeTab === "favorites" && !config.favorites.includes(srv.id)) return false;
    if (searchQuery && !srv.name.toLowerCase().includes(searchQuery.toLowerCase()) && !srv.ip.includes(searchQuery)) return false;
    if (selectedMap !== "ALL" && srv.map.toLowerCase() !== selectedMap.toLowerCase()) return false;
    if (srv.ping > maxPing) return false;
    if (modFilter === "MODDED" && !srv.is_modded) return false;
    if (modFilter === "VANILLA" && srv.is_modded) return false;
    return true;
  });

  return (
    <div style={{ display: "flex", height: "100vh", width: "100vw", background: "var(--bg-dark)" }}>
      
      {/* SIDEBAR NAVIGATION */}
      <aside style={{ width: "230px", background: "var(--bg-sidebar)", borderRight: "1px solid var(--border-color)", display: "flex", flexDirection: "column" }}>
        
        {/* LOGO */}
        <div style={{ padding: "20px 16px", display: "flex", alignItems: "center", gap: "10px", borderBottom: "1px solid var(--border-color)" }}>
          <div style={{ width: "36px", height: "36px", borderRadius: "10px", background: "linear-gradient(135deg, #ff3b30, #b30000)", display: "flex", alignItems: "center", justifyContent: "center", boxShadow: "0 0 16px rgba(255, 59, 48, 0.4)" }}>
            <Flame size={22} color="#fff" />
          </div>
          <div>
            <h1 style={{ fontSize: "18px", fontWeight: 800, letterSpacing: "-0.5px", background: "linear-gradient(90deg, #fff, #a0aec0)", WebkitBackgroundClip: "text", WebkitTextFillColor: "transparent" }}>
              ApexDZ
            </h1>
            <span style={{ fontSize: "10px", color: "var(--accent-red)", fontWeight: 700, letterSpacing: "1px" }}>DAYZ LINUX</span>
          </div>
        </div>

        {/* NAVIGATION ITEMS */}
        <nav style={{ padding: "16px 10px", display: "flex", flexDirection: "column", gap: "4px", flex: 1 }}>
          <button 
            onClick={() => setActiveTab("servers")}
            style={{ display: "flex", alignItems: "center", gap: "12px", padding: "10px 14px", borderRadius: "8px", border: "none", background: activeTab === "servers" ? "rgba(255, 59, 48, 0.15)" : "transparent", color: activeTab === "servers" ? "#fff" : "var(--text-muted)", fontWeight: activeTab === "servers" ? 600 : 400, cursor: "pointer", textAlign: "left", transition: "all 0.15s" }}>
            <Server size={18} color={activeTab === "servers" ? "var(--accent-red)" : "inherit"} />
            Servidores
          </button>

          <button 
            onClick={() => setActiveTab("favorites")}
            style={{ display: "flex", alignItems: "center", gap: "12px", padding: "10px 14px", borderRadius: "8px", border: "none", background: activeTab === "favorites" ? "rgba(255, 149, 0, 0.15)" : "transparent", color: activeTab === "favorites" ? "#fff" : "var(--text-muted)", fontWeight: activeTab === "favorites" ? 600 : 400, cursor: "pointer", textAlign: "left", transition: "all 0.15s" }}>
            <Star size={18} color={activeTab === "favorites" ? "var(--accent-orange)" : "inherit"} />
            Favoritos ({config.favorites.length})
          </button>

          <button 
            onClick={() => setActiveTab("history")}
            style={{ display: "flex", alignItems: "center", gap: "12px", padding: "10px 14px", borderRadius: "8px", border: "none", background: activeTab === "history" ? "rgba(10, 132, 255, 0.15)" : "transparent", color: activeTab === "history" ? "#fff" : "var(--text-muted)", fontWeight: activeTab === "history" ? 600 : 400, cursor: "pointer", textAlign: "left", transition: "all 0.15s" }}>
            <History size={18} color={activeTab === "history" ? "var(--accent-blue)" : "inherit"} />
            Historial
          </button>

          <button 
            onClick={() => setActiveTab("mods")}
            style={{ display: "flex", alignItems: "center", gap: "12px", padding: "10px 14px", borderRadius: "8px", border: "none", background: activeTab === "mods" ? "rgba(48, 209, 88, 0.15)" : "transparent", color: activeTab === "mods" ? "#fff" : "var(--text-muted)", fontWeight: activeTab === "mods" ? 600 : 400, cursor: "pointer", textAlign: "left", transition: "all 0.15s" }}>
            <FolderCheck size={18} color={activeTab === "mods" ? "var(--accent-green)" : "inherit"} />
            Mods ({installedMods.length})
          </button>

          <button 
            onClick={() => setActiveTab("settings")}
            style={{ display: "flex", alignItems: "center", gap: "12px", padding: "10px 14px", borderRadius: "8px", border: "none", background: activeTab === "settings" ? "rgba(255, 255, 255, 0.1)" : "transparent", color: activeTab === "settings" ? "#fff" : "var(--text-muted)", fontWeight: activeTab === "settings" ? 600 : 400, cursor: "pointer", textAlign: "left", transition: "all 0.15s" }}>
            <SettingsIcon size={18} color={activeTab === "settings" ? "#fff" : "inherit"} />
            Configuración
          </button>
        </nav>

        {/* SYSTEM STATUS SUMMARY */}
        <div style={{ padding: "14px", margin: "10px", background: "rgba(0, 0, 0, 0.4)", borderRadius: "8px", border: "1px solid var(--border-color)", fontSize: "11px" }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "6px" }}>
            <span style={{ color: "var(--text-muted)" }}>Steam System:</span>
            <span style={{ fontWeight: 600, color: steamEnv?.is_flatpak ? "var(--accent-cyan)" : "var(--accent-green)" }}>
              {steamEnv?.is_flatpak ? "Flatpak" : "Nativo"}
            </span>
          </div>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
            <span style={{ color: "var(--text-muted)" }}>DayZ Detected:</span>
            <span style={{ fontWeight: 600, color: steamEnv?.dayz_found !== false ? "var(--accent-green)" : "var(--accent-red)" }}>
              {steamEnv?.dayz_found !== false ? "OK" : "No detectado"}
            </span>
          </div>
        </div>
      </aside>

      {/* MAIN CONTENT AREA */}
      <main style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
        
        {/* HEADER BAR */}
        <header className="glass-header" style={{ height: "64px", padding: "0 24px", display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          
          {/* PLAYER NAME INPUT */}
          <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
            <span style={{ fontSize: "12px", color: "var(--text-muted)", fontWeight: 500 }}>Nombre en Juego:</span>
            <input 
              type="text" 
              value={config.player_name}
              onChange={(e) => handleSaveConfig({ ...config, player_name: e.target.value })}
              placeholder="Survivor"
              style={{ width: "160px", background: "rgba(0, 0, 0, 0.4)", border: "1px solid var(--border-color)", color: "#fff", fontWeight: 600 }}
            />
          </div>

          {/* LAUNCH STATUS NOTIFICATION */}
          {launchStatus && (
            <div style={{ padding: "6px 14px", background: "rgba(255, 59, 48, 0.2)", border: "1px solid var(--accent-red)", borderRadius: "6px", fontSize: "12px", color: "#fff", display: "flex", alignItems: "center", gap: "8px" }}>
              <AlertCircle size={16} color="var(--accent-red)" />
              {launchStatus}
            </div>
          )}

          {/* QUICK DIRECT CONNECT */}
          <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
            <button 
              onClick={() => {
                const ip = prompt("Introduce IP:Puerto del servidor (ej. 192.168.1.50:2302)");
                if (ip) {
                  callDesktopIpc("launch_game", {
                    ipPort: ip,
                    modIds: [],
                    customName: config.player_name,
                    useGamemode: config.use_gamemode,
                    useMangohud: config.use_mangohud,
                    customArgs: config.custom_launch_params
                  });
                }
              }}
              style={{ display: "flex", alignItems: "center", gap: "8px", padding: "8px 14px", background: "rgba(255, 255, 255, 0.08)", border: "1px solid var(--border-color)", borderRadius: "6px", color: "#fff", fontSize: "12px", cursor: "pointer" }}>
              <Terminal size={14} /> Conexión Directa
            </button>
          </div>
        </header>

        {/* BODY TAB CONTENT */}
        <div style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column", padding: "20px 24px" }}>
          
          {/* SEARCH & FILTER BAR FOR SERVERS */}
          {(activeTab === "servers" || activeTab === "favorites") && (
            <div style={{ display: "flex", alignItems: "center", gap: "12px", marginBottom: "16px", flexWrap: "wrap" }}>
              
              {/* SEARCH INPUT */}
              <div style={{ flex: 1, minWidth: "260px", position: "relative" }}>
                <Search size={16} color="var(--text-muted)" style={{ position: "absolute", left: "12px", top: "50%", transform: "translateY(-50%)" }} />
                <input 
                  type="text" 
                  placeholder="Buscar servidor por nombre, IP o mapa..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  style={{ width: "100%", paddingLeft: "36px" }}
                />
              </div>

              {/* MAP SELECTOR */}
              <select value={selectedMap} onChange={(e) => setSelectedMap(e.target.value)}>
                <option value="ALL">🗺️ Todos los Mapas</option>
                <option value="Chernarus">Chernarus</option>
                <option value="Livonia">Livonia</option>
                <option value="Sakhal">Sakhal (Frostline)</option>
                <option value="Namalsk">Namalsk</option>
                <option value="DeerIsle">DeerIsle</option>
              </select>

              {/* MOD FILTER */}
              <select value={modFilter} onChange={(e) => setModFilter(e.target.value as any)}>
                <option value="ALL">🧩 Mods: Todos</option>
                <option value="MODDED">Con Mods</option>
                <option value="VANILLA">Vanilla (Sin Mods)</option>
              </select>

              {/* MAX PING */}
              <div style={{ display: "flex", alignItems: "center", gap: "8px", fontSize: "12px", color: "var(--text-muted)" }}>
                <span>Ping Máx: {maxPing}ms</span>
                <input 
                  type="range" 
                  min={30} 
                  max={300} 
                  step={10} 
                  value={maxPing} 
                  onChange={(e) => setMaxPing(Number(e.target.value))} 
                  style={{ width: "90px" }}
                />
              </div>
            </div>
          )}

          {/* SERVERS / FAVORITES TABLE VIEW */}
          {(activeTab === "servers" || activeTab === "favorites") && (
            <div className="glass" style={{ flex: 1, borderRadius: "12px", overflow: "hidden", display: "flex", flexDirection: "column" }}>
              <div style={{ padding: "12px 20px", background: "rgba(0,0,0,0.3)", borderBottom: "1px solid var(--border-color)", display: "grid", gridTemplateColumns: "40px 1fr 120px 90px 80px 100px 110px", fontWeight: 600, fontSize: "11px", color: "var(--text-muted)", textTransform: "uppercase" }}>
                <span>Fav</span>
                <span>Nombre del Servidor</span>
                <span>Mapa</span>
                <span>Jugadores</span>
                <span>Ping</span>
                <span>Mods</span>
                <span>Acción</span>
              </div>

              <div style={{ flex: 1, overflowY: "auto" }}>
                {filteredServers.length === 0 ? (
                  <div style={{ padding: "40px", textAlign: "center", color: "var(--text-muted)" }}>
                    No se encontraron servidores con los filtros seleccionados.
                  </div>
                ) : (
                  filteredServers.map((srv) => (
                    <div 
                      key={srv.id}
                      onClick={() => handleSelectServer(srv)}
                      style={{ 
                        display: "grid", 
                        gridTemplateColumns: "40px 1fr 120px 90px 80px 100px 110px", 
                        padding: "14px 20px", 
                        borderBottom: "1px solid var(--border-color)", 
                        alignItems: "center",
                        background: selectedServer?.id === srv.id ? "rgba(255, 59, 48, 0.08)" : "transparent",
                        cursor: "pointer",
                        transition: "background 0.15s"
                      }}
                      onMouseEnter={(e) => e.currentTarget.style.background = "var(--bg-card-hover)"}
                      onMouseLeave={(e) => e.currentTarget.style.background = selectedServer?.id === srv.id ? "rgba(255, 59, 48, 0.08)" : "transparent"}
                    >
                      {/* FAVORITE STAR */}
                      <button 
                        onClick={(e) => { e.stopPropagation(); toggleFavorite(srv.id); }}
                        style={{ background: "none", border: "none", cursor: "pointer" }}>
                        <Star size={16} color={config.favorites.includes(srv.id) ? "var(--accent-orange)" : "var(--text-dim)"} fill={config.favorites.includes(srv.id) ? "var(--accent-orange)" : "none"} />
                      </button>

                      {/* SERVER NAME & IP */}
                      <div>
                        <div style={{ fontWeight: 600, fontSize: "13px", color: "#fff", display: "flex", alignItems: "center", gap: "6px" }}>
                          {srv.name}
                          {!srv.is_3pp && <span className="badge badge-red">1PP</span>}
                          {srv.has_password && <Lock size={12} color="var(--accent-orange)" />}
                        </div>
                        <div style={{ fontSize: "11px", color: "var(--text-dim)", fontFamily: "monospace" }}>{srv.ip}:{srv.port}</div>
                      </div>

                      {/* MAP */}
                      <div style={{ fontSize: "12px", color: "var(--accent-cyan)", fontWeight: 500 }}>
                        {srv.map}
                      </div>

                      {/* PLAYERS */}
                      <div style={{ fontSize: "12px", fontWeight: 600 }}>
                        <span style={{ color: srv.players >= srv.max_players ? "var(--accent-red)" : "#fff" }}>{srv.players}</span>
                        <span style={{ color: "var(--text-dim)" }}>/{srv.max_players}</span>
                      </div>

                      {/* PING */}
                      <div style={{ fontSize: "12px", fontWeight: 700 }} className={srv.ping < 50 ? "ping-good" : srv.ping < 120 ? "ping-ok" : "ping-bad"}>
                        {srv.ping} ms
                      </div>

                      {/* MODS BADGE */}
                      <div>
                        {srv.is_modded ? (
                          <span className="badge badge-blue">{srv.mods_count} Mods</span>
                        ) : (
                          <span className="badge badge-green">Vanilla</span>
                        )}
                      </div>

                      {/* JOIN BUTTON */}
                      <button 
                        className="glow-button"
                        onClick={(e) => { e.stopPropagation(); handleLaunchServer(srv); }}
                        style={{ padding: "6px 12px", fontSize: "12px", display: "flex", alignItems: "center", gap: "6px", justifyContent: "center" }}>
                        <Play size={12} fill="#fff" /> Entrar
                      </button>
                    </div>
                  ))
                )}
              </div>
            </div>
          )}

          {/* MODS MANAGER TAB */}
          {activeTab === "mods" && (
            <div className="glass" style={{ flex: 1, borderRadius: "12px", padding: "24px", display: "flex", flexDirection: "column", gap: "16px" }}>
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                <h2 style={{ fontSize: "18px", fontWeight: 700 }}>Gestor de Mods del Workshop de DayZ</h2>
                <button 
                  onClick={() => callDesktopIpc<InstalledModInfo[]>("get_installed_workshop_mods").then((mods) => Array.isArray(mods) && setInstalledMods(mods))}
                  style={{ display: "flex", alignItems: "center", gap: "6px", padding: "8px 14px", background: "rgba(255, 255, 255, 0.08)", border: "1px solid var(--border-color)", borderRadius: "6px", color: "#fff", cursor: "pointer" }}>
                  <RefreshCw size={14} /> Recargar Lista
                </button>
              </div>

              <div style={{ flex: 1, overflowY: "auto", display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))", gap: "12px" }}>
                {installedMods.length === 0 ? (
                  <div style={{ color: "var(--text-muted)", gridColumn: "1/-1", textAlign: "center", padding: "40px" }}>
                    No se han encontrado mods descargados en la carpeta del Workshop de Steam (`steamapps/workshop/content/221100`).
                  </div>
                ) : (
                  installedMods.map((mod) => (
                    <div key={mod.id} style={{ background: "rgba(0, 0, 0, 0.3)", border: "1px solid var(--border-color)", borderRadius: "8px", padding: "14px" }}>
                      <div style={{ fontWeight: 600, fontSize: "14px", color: "#fff", marginBottom: "4px" }}>{mod.name}</div>
                      <div style={{ fontSize: "11px", color: "var(--text-dim)", fontFamily: "monospace" }}>ID Workshop: {mod.id}</div>
                    </div>
                  ))
                )}
              </div>
            </div>
          )}

          {/* SETTINGS TAB */}
          {activeTab === "settings" && (
            <div className="glass" style={{ flex: 1, borderRadius: "12px", padding: "24px", maxWidth: "650px", overflowY: "auto" }}>
              <h2 style={{ fontSize: "18px", fontWeight: 700, marginBottom: "20px" }}>Configuración de Lanzamiento de Linux</h2>

              <div style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
                
                {/* GAME MODE & MANGO HUD */}
                <div>
                  <h3 style={{ fontSize: "14px", fontWeight: 600, marginBottom: "10px", color: "var(--accent-cyan)" }}>Optimizaciones de Juego en Linux</h3>
                  <label style={{ display: "flex", alignItems: "center", gap: "10px", cursor: "pointer", marginBottom: "8px" }}>
                    <input 
                      type="checkbox" 
                      checked={config.use_gamemode} 
                      onChange={(e) => handleSaveConfig({ ...config, use_gamemode: e.target.checked })} 
                    />
                    <span>Activar <strong>GameMode</strong> (`gamemoderun`) para optimizar rendimiento de CPU/GPU</span>
                  </label>

                  <label style={{ display: "flex", alignItems: "center", gap: "10px", cursor: "pointer" }}>
                    <input 
                      type="checkbox" 
                      checked={config.use_mangohud} 
                      onChange={(e) => handleSaveConfig({ ...config, use_mangohud: e.target.checked })} 
                    />
                    <span>Activar <strong>MangoHud</strong> (Overlay de FPS, temperaturas y rendimiento)</span>
                  </label>
                </div>

                {/* CUSTOM LAUNCH PARAMETERS */}
                <div>
                  <h3 style={{ fontSize: "14px", fontWeight: 600, marginBottom: "8px", color: "var(--accent-cyan)" }}>Parámetros Personalizados de DayZ</h3>
                  <input 
                    type="text" 
                    value={config.custom_launch_params} 
                    onChange={(e) => handleSaveConfig({ ...config, custom_launch_params: e.target.value })} 
                    style={{ width: "100%", fontFamily: "monospace" }}
                  />
                  <span style={{ fontSize: "11px", color: "var(--text-dim)", marginTop: "4px", display: "block" }}>
                    Ejemplo: `-nolauncher -world=empty -nosplash -cpuCount=8`
                  </span>
                </div>

              </div>
            </div>
          )}

        </div>
      </main>

      {/* DETAILED SERVER MODAL */}
      {selectedServer && (
        <div style={{ position: "fixed", inset: 0, background: "rgba(0,0,0,0.75)", backdropFilter: "blur(6px)", display: "flex", alignItems: "center", justifyContent: "center", zIndex: 1000 }}>
          <div className="glass" style={{ width: "550px", borderRadius: "14px", padding: "24px", display: "flex", flexDirection: "column", gap: "16px" }}>
            <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between" }}>
              <div>
                <h3 style={{ fontSize: "16px", fontWeight: 700, color: "#fff" }}>{selectedServer.name}</h3>
                <span style={{ fontSize: "12px", color: "var(--accent-cyan)" }}>{selectedServer.ip}:{selectedServer.port} • Mapa: {selectedServer.map}</span>
              </div>
              <button 
                onClick={() => setSelectedServer(null)}
                style={{ background: "none", border: "none", color: "var(--text-muted)", fontSize: "18px", cursor: "pointer" }}>✕</button>
            </div>

            <div style={{ borderTop: "1px solid var(--border-color)", paddingTop: "14px" }}>
              <div style={{ fontSize: "13px", fontWeight: 600, marginBottom: "8px", color: "var(--text-muted)" }}>
                Lista de Mods requeridos por el servidor ({serverModalMods.length}):
              </div>

              <div style={{ maxHeight: "220px", overflowY: "auto", display: "flex", flexDirection: "column", gap: "6px" }}>
                {loadingDzsa ? (
                  <div style={{ color: "var(--text-muted)", fontSize: "12px" }}>Consultando API de DZSA Launcher...</div>
                ) : serverModalMods.length === 0 ? (
                  <div style={{ color: "var(--accent-green)", fontSize: "12px" }}>Este servidor es Vanilla (No requiere ningún mod).</div>
                ) : (
                  serverModalMods.map((m, idx) => (
                    <div key={idx} style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "6px 10px", background: "rgba(0,0,0,0.3)", borderRadius: "6px", fontSize: "12px" }}>
                      <span style={{ color: "#fff", fontWeight: 500 }}>{m.name}</span>
                      <a 
                        href={`https://steamcommunity.com/sharedfiles/filedetails/?id=${m.workshop_id}`} 
                        target="_blank" 
                        rel="noreferrer"
                        style={{ color: "var(--accent-cyan)", textDecoration: "none", display: "flex", alignItems: "center", gap: "4px" }}>
                        ID: {m.workshop_id} <ExternalLink size={12} />
                      </a>
                    </div>
                  ))
                )}
              </div>
            </div>

            <div style={{ display: "flex", justifyContent: "flex-end", gap: "10px", borderTop: "1px solid var(--border-color)", paddingTop: "14px" }}>
              <button 
                onClick={() => setSelectedServer(null)}
                style={{ padding: "8px 16px", background: "rgba(255,255,255,0.08)", border: "1px solid var(--border-color)", borderRadius: "6px", color: "#fff", cursor: "pointer" }}>
                Cerrar
              </button>
              <button 
                className="glow-button"
                onClick={() => { handleLaunchServer(selectedServer); setSelectedServer(null); }}
                style={{ padding: "8px 20px" }}>
                🚀 Conectar y Lanzar DayZ
              </button>
            </div>
          </div>
        </div>
      )}

    </div>
  );
}
