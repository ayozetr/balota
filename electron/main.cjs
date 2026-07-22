const { app, BrowserWindow, ipcMain } = require('electron');
const path = require('path');
const fs = require('fs');
const { spawn, execSync } = require('child_process');
const os = require('os');

const DAYZ_APP_ID = "221100";
const FLATPAK_STEAM_ID = "com.valvesoftware.Steam";

function getSteamEnv() {
  const home = os.homedir();
  
  // 1. Native Steam
  const nativeRoot = path.join(home, ".local/share/Steam");
  const nativeDayZ = path.join(nativeRoot, "steamapps/common/DayZ");
  const nativeWorkshop = path.join(nativeRoot, `steamapps/workshop/content/${DAYZ_APP_ID}`);

  if (fs.existsSync(nativeDayZ)) {
    return {
      is_flatpak: false,
      steam_root: nativeRoot,
      dayz_dir: nativeDayZ,
      workshop_dir: nativeWorkshop,
      dayz_found: true,
    };
  }

  // 2. Flatpak Steam
  const flatpakRoot = path.join(home, `.var/app/${FLATPAK_STEAM_ID}/data/Steam`);
  const flatpakDayZ = path.join(flatpakRoot, "steamapps/common/DayZ");
  const flatpakWorkshop = path.join(flatpakRoot, `steamapps/workshop/content/${DAYZ_APP_ID}`);

  if (fs.existsSync(flatpakDayZ)) {
    return {
      is_flatpak: true,
      steam_root: flatpakRoot,
      dayz_dir: flatpakDayZ,
      workshop_dir: flatpakWorkshop,
      dayz_found: true,
    };
  }

  return {
    is_flatpak: false,
    steam_root: nativeRoot,
    dayz_dir: nativeDayZ,
    workshop_dir: nativeWorkshop,
    dayz_found: false,
  };
}

function modIdToBase64Symlink(modId) {
  const buf = Buffer.from(modId, 'utf-8');
  const b64 = buf.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
  return `@${b64}`;
}

function prepareModSymlinks(dayzDir, workshopDir, modIds) {
  if (!fs.existsSync(dayzDir)) return [];
  const symlinks = [];

  for (const modId of modIds) {
    const modPath = path.join(workshopDir, modId);
    if (!fs.existsSync(modPath)) continue;

    const symlinkName = modIdToBase64Symlink(modId);
    const targetLink = path.join(dayzDir, symlinkName);

    if (!fs.existsSync(targetLink)) {
      try {
        fs.symlinkSync(modPath, targetLink, 'dir');
        console.log(`[ApexDZ Electron] Created symlink ${symlinkName} -> ${modPath}`);
      } catch (e) {
        console.warn(`[ApexDZ Electron] Symlink error for ${modId}:`, e);
      }
    }
    symlinks.push(symlinkName);
  }

  return symlinks;
}

function getInstalledMods(workshopDir) {
  if (!fs.existsSync(workshopDir)) return [];
  const mods = [];

  try {
    const entries = fs.readdirSync(workshopDir);
    for (const id of entries) {
      const modPath = path.join(workshopDir, id);
      if (fs.statSync(modPath).isDirectory()) {
        let name = `Mod ${id}`;
        const metaPath = path.join(modPath, "meta.cpp");
        if (fs.existsSync(metaPath)) {
          try {
            const metaStr = fs.readFileSync(metaPath, "utf-8");
            const match = metaStr.match(/name\s*=\s*"([^"]+)"/);
            if (match && match[1]) name = match[1];
          } catch (_) {}
        }
        mods.push({
          id,
          name,
          path: modPath,
          size_bytes: 0
        });
      }
    }
  } catch (e) {
    console.error("Error listing mods:", e);
  }

  return mods;
}

// IPC Handlers
ipcMain.handle('get_steam_environment', () => getSteamEnv());
ipcMain.handle('get_installed_workshop_mods', () => {
  const env = getSteamEnv();
  return getInstalledMods(env.workshop_dir);
});

ipcMain.handle('query_dzsa', async (_, { ip, queryPort }) => {
  const url = `https://dayzsalauncher.com/api/v1/query/${ip}/${queryPort}`;
  const res = await fetch(url, { headers: { 'User-Agent': 'ApexDZ-Linux-Launcher/1.0' } });
  return await res.json();
});

ipcMain.handle('launch_game', (_, { ipPort, modIds, customName, useGamemode, useMangohud, customArgs }) => {
  const env = getSteamEnv();
  if (!env.dayz_found) {
    throw new Error("Directorio de instalación de DayZ no encontrado.");
  }

  const symlinks = prepareModSymlinks(env.dayz_dir, env.workshop_dir, modIds || []);
  const cmdArgs = [];

  if (env.is_flatpak) {
    cmdArgs.push("run", "--branch=stable", "--arch=x86_64", "--command=/app/bin/steam-wrapper", FLATPAK_STEAM_ID, "-applaunch", DAYZ_APP_ID);
  } else {
    cmdArgs.push("-applaunch", DAYZ_APP_ID);
  }

  if (symlinks.length > 0) {
    cmdArgs.push(`-mod=${symlinks.join(";")}`);
  }

  if (ipPort) {
    cmdArgs.push(`-connect=${ipPort}`, "-nolauncher", "-world=empty");
  }

  if (customName) {
    cmdArgs.push(`-name=${customName}`);
  }

  if (customArgs) {
    cmdArgs.push(...customArgs.split(' '));
  }

  let bin = env.is_flatpak ? "flatpak" : "steam";
  if (useGamemode) {
    cmdArgs.unshift(bin);
    bin = "gamemoderun";
  } else if (useMangohud) {
    cmdArgs.unshift(bin);
    bin = "mangohud";
  }

  console.log(`[ApexDZ Launch] Spawning: ${bin}`, cmdArgs);
  spawn(bin, cmdArgs, { detached: true, stdio: 'ignore' }).unref();
  return `DayZ lanzado correctamente con ${bin}!`;
});

function createWindow() {
  const win = new BrowserWindow({
    width: 1280,
    height: 800,
    title: "ApexDZ - DayZ Linux Launcher",
    backgroundColor: "#0b0f19",
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false
    }
  });

  const distIndex = path.join(__dirname, '../dist/index.html');
  if (fs.existsSync(distIndex)) {
    win.loadFile(distIndex);
  } else {
    win.loadURL('http://localhost:1420');
  }
}

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
