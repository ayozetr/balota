# 🐺 DayZ Modern Linux Launcher (ApexDZ)

Un launcher nativo, moderno y de alto rendimiento para **DayZ en Linux y Steam Deck**, diseñado para reemplazar a DZSA Launcher en Linux con la máxima velocidad, integración con Steam/Proton y soporte de mods con un solo clic.

---

## 📚 Documentación del Proyecto
- 📘 **[DEVELOPMENT.md](DEVELOPMENT.md)**: Guía completa de desarrollo, puesta en marcha en un nuevo equipo, arquitectura técnica y hoja de ruta futura.
- ⚙️ **[apexdz-cli.sh](apexdz-cli.sh)**: Lanzador de consola en Bash.

---

## 🚀 Inicio Rápido (Quick Start)

Para ejecutar el proyecto en cualquier equipo Linux:

```bash
cd /home/ayoze/Proyectos/dayz-linux-launcher
npm install
npm start
```

---

## 🔍 Resumen del Proyecto

### 1. ¿Cómo resuelve los mods de DayZ en Linux?
ApexDZ genera enlaces simbólicos en la carpeta del juego (`common/DayZ/@<BASE64_ID>`) apuntando a la carpeta de descarga de Steam Workshop (`workshop/content/221100/<WORKSHOP_ID>`). De esta forma se evitan fallos de codificación de caracteres bajo Proton/Wine.

### 2. Consulta de Servidores
- **API DZSA**: `https://dayzsalauncher.com/api/v1/query/<IP>/<PUERTO>`
- **Valve A2S UDP**: Medición de latencia real en milisegundos mediante sockets UDP.

### 3. Integración con Linux Gaming
- Soporte automático para **Steam Nativo** y **Steam Flatpak**.
- Conmutadores en 1-click para activar **GameMode (`gamemoderun`)** y **MangoHud**.

---

## 🗺️ Próximos Pasos (Roadmap)
Consulta **[DEVELOPMENT.md](DEVELOPMENT.md)** para ver la lista detallada de tareas pendientes (auto-suscripción de mods, lista maestra de servidores globales, soporte para Steam Deck en modo gamepad y empaquetado AppImage/Flatpak).
