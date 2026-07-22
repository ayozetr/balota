# 🛠️ Guía de Desarrollo y Hoja de Ruta (ApexDZ)

Esta guía contiene **toda la información técnica, arquitectura, protocolo de DayZ en Linux y planes futuros** necesaria para que cualquier desarrollador pueda continuar el proyecto en otro equipo o entorno.

---

## 📌 1. Requisitos del Sistema para Desarrolladores

Para clonar, ejecutar y compilar este proyecto en un nuevo equipo con cualquier distribución Linux (Ubuntu, Debian, Fedora, Arch Linux, openSUSE, SteamOS):

### Requisitos Básicos:
- **Node.js**: v18.0.0 o superior (verificado con v24.16)
- **NPM**: v9.0.0 o superior (verificado con v12.0)
- **Git**

### Requisitos Opcionales (para compilar con Tauri/Rust nativo):
- **Rust & Cargo**: v1.75+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- **Librerías de desarrollo C/GTK** (solo si se compila el ejecutable Tauri):
  - Arch: `sudo pacman -S webkit2gtk-4.1 gtk3 pkg-config`
  - Ubuntu/Debian: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev build-essential curl wget file libssl-dev libayatana-appindicator3-dev librsvg2-dev`
  - Fedora: `sudo dnf install webkit2gtk4.1-devel gtk3-devel openssl-devel gcc-c++`

---

## 🚀 2. Instalación y Puesta en Marcha en un Nuevo Equipo

1. **Clonar o copiar el proyecto** al nuevo equipo:
   ```bash
   git clone <url-del-repositorio> apexdz-launcher
   cd apexdz-launcher
   ```

2. **Instalar dependencias de Node.js**:
   ```bash
   npm install
   ```

3. **Ejecutar en modo Desarrollo (Desktop / Electron)**:
   ```bash
   npm start
   ```

4. **Compilar los bundles de distribución**:
   ```bash
   npm run build
   ```

---

## 📐 3. Estructura y Arquitectura del Proyecto

```
dayz-linux-launcher/
├── electron/
│   └── main.cjs          # Runner de escritorio portable (IPC, ejecutor Steam, simlinks)
├── src-tauri/            # Backend nativo en Rust (A2S UDP pinger, DZSA API, Tauri IPC)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs       # Entrypoint IPC de Tauri
│       ├── steam.rs      # Detección de Steam/Flatpak y creador de enlaces simbólicos @base64
│       ├── dzsa.rs       # Cliente API DZSA y socket UDP Valve A2S
│       └── config.rs     # Gestor de configuración (~/.config/apexdz/config.json)
├── src/                  # Frontend en React 18 + Tailwind CSS / CSS Moderno
│   ├── App.tsx           # Layout principal, estado global, filtros y modal de servidores
│   ├── index.css         # Sistema de diseño, temas glassmorphism, colores de latencia
│   ├── main.tsx          # Punto de entrada de React
│   └── types.ts          # Interfaces de TypeScript (ServerItem, InstalledModInfo, etc.)
├── apexdz-cli.sh         # Script ejecutable companion para consola Bash
├── README.md             # Visión general del proyecto y protocolos
└── DEVELOPMENT.md        # Esta guía de desarrollo y hoja de ruta futura
```

---

## ⚙️ 4. Protocolos y Funcionamiento Interno de DayZ en Linux

### A. Simlinks de Mods y Base64 (`@<base64_mod_id>`)
DayZ ejecutado mediante Proton en Linux no tolera caracteres especiales ni espacios en los parámetros `-mod=...`.
Para resolver esto de forma segura:
1. Los mods se descargan en `steamapps/workshop/content/221100/<WORKSHOP_ID>`.
2. ApexDZ crea un symlink en `steamapps/common/DayZ/@<BASE64_ID>`.
3. El ID del mod (ej. `1564026768`) se convierte a Base64 sin caracteres conflictivos.
4. Se pasa a Steam el parámetro: `-mod=@<BASE64_ID1>;@<BASE64_ID2>`.

### B. Consulta de Servidores (API DZSA & Valve A2S)
- **API DZSA**: `https://dayzsalauncher.com/api/v1/query/<IP>/<QUERY_PORT>`
  - Devuelve JSON con la lista de mods requeridos (`steamWorkshopId` y `name`).
- **Valve A2S UDP**:
  - Se envía paquete A2S_INFO (`0xFF 0xFF 0xFF 0xFF 'T' Source Engine Query\0`) por socket UDP al puerto de query (por defecto `27016`).
  - Mide la latencia de red exacta en milisegundos.

---

## 🗺️ 5. Hoja de Ruta y Tareas Pendientes (Roadmap Futuro)

Si otra persona o tú continúan el desarrollo en el futuro, estas son las funcionalidades planeadas para las siguientes versiones:

### 🔹 Fase 1: Descarga e Instalación Automática de Mods de Workshop (Prioridad Alta)
- [ ] **Suscripción vía Protocolo de Steam**: Abrir automáticamente enlaces `steam://url/CommunityFilePage/<mod_id>` cuando falten mods al intentar entrar a un servidor.
- [ ] **Modo Headless con SteamCMD**: Permitir al usuario introducir sus credenciales de SteamCMD para pre-descargar mods en segundo plano sin abrir la ventana del cliente de Steam.

### 🔹 Fase 2: Buscador Maestro de Servidores Globales (Prioridad Media)
- [ ] **Lista Maestra de Servidores**: Integrar consulta al Valve Master Server API o BattleMetrics API para rellenar automáticamente la lista con más de 5.000 servidores activos de DayZ en todo el mundo.
- [ ] **Servicio de Pings Asíncronos masivos**: Usar el motor `tokio::net::UdpSocket` de Rust en `src-tauri/src/dzsa.rs` para escanear pings de 1.000 servidores por segundo.

### 🔹 Fase 3: Optimización para Steam Deck & Modo Mando (Prioridad Media)
- [ ] **Navegación por Gamepad**: Añadir soporte completo para D-Pad / Joysticks para navegar cómodamente en la interfaz de Steam Deck en Game Mode.
- [ ] **Preset de Gamescope**: Añadir conmutadores para limitar FPS y resolución en Steam Deck.

### 🔹 Fase 4: Integración Social y Discord Rich Presence (Prioridad Baja)
- [ ] **Discord Rich Presence**: Mostrar en Discord el servidor al que estás conectado (ej. *"Jugando en DayZ Underground - 54/60 Jugadores"*).
- [ ] **Exportar/Importar Favoritos**: Permitir guardar y restaurar servidores favoritos en un archivo JSON.

### 🔹 Fase 5: Empaquetado y Distribución
- [ ] **Creación de AppImage**: Generar ejecutable portable `.AppImage` usando Tauri o Electron-Builder.
- [ ] **Publicación en Flathub (Flatpak)**: Crear manifiesto Flatpak para distribución directa en tiendas de software como Discover / GNOME Software.
