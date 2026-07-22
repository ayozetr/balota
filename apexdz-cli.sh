#!/usr/bin/env bash
# ApexDZ - DayZ Linux CLI Launcher Companion
set -eo pipefail

DAYZ_ID=221100
DEFAULT_GAMEPORT=2302
DEFAULT_QUERYPORT=27016

print_help() {
  cat <<EOF
ApexDZ - DayZ Linux Launcher CLI
Uso: ./apexdz-cli.sh -s <IP:PUERTO> [-n <NOMBRE_JUGADOR>]

Opciones:
  -s, --server <IP[:PORT]>   Servidor al que conectar (API DZSA Launcher)
  -n, --name <NAME>          Nombre del perfil de jugador
  -l, --launch               Lanzar DayZ automáticamente vía Steam
  -h, --help                 Mostrar esta ayuda
EOF
}

SERVER=""
NAME=""
LAUNCH=0

while (( "$#" )); do
  case "${1}" in
    -s|--server) SERVER="${2}"; shift ;;
    -n|--name) NAME="${2}"; shift ;;
    -l|--launch) LAUNCH=1 ;;
    -h|--help) print_help; exit 0 ;;
  esac
  shift
done

if [[ -z "${SERVER}" ]]; then
  echo "[ApexDZ CLI] Especifica un servidor con -s <IP:PUERTO>"
  exit 1
fi

echo "[ApexDZ CLI] Consultando API DZSA Launcher para ${SERVER}..."
API_URL="https://dayzsalauncher.com/api/v1/query/${SERVER%:*}/${DEFAULT_QUERYPORT}"
RESPONSE=$(curl -sSL -m 8 "${API_URL}" || echo "")

if [[ -n "${RESPONSE}" ]]; then
  MODS_JSON=$(echo "${RESPONSE}" | jq -r '.result.mods[]?.steamWorkshopId' 2>/dev/null || echo "")
  echo "[ApexDZ CLI] Mods detectados para el servidor:"
  echo "${MODS_JSON}"
fi

if [[ "${LAUNCH}" == 1 ]]; then
  echo "[ApexDZ CLI] Lanzando Steam..."
  steam -applaunch "${DAYZ_ID}" -connect="${SERVER}" -name="${NAME:-Survivor}" -nolauncher -world=empty
fi
