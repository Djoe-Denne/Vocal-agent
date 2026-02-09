#!/bin/bash
# Nettoyage complet OpenClaw + Podman

echo "=== 1. Arrêt et suppression du container Podman ==="
sudo podman stop openclaw-agent 2>/dev/null || true
sudo podman rm -f openclaw-agent 2>/dev/null || true

echo "=== 2. Suppression de l'image Podman ==="
sudo podman rmi -f openclaw-agent 2>/dev/null || true

echo "=== 3. Vérification des images restantes ==="
sudo podman images | grep openclaw

echo "=== 4. Arrêt du service systemd OpenClaw (si existe) ==="
sudo systemctl stop openclaw 2>/dev/null || true
sudo systemctl stop openclaw-gateway 2>/dev/null || true
sudo systemctl disable openclaw 2>/dev/null || true
sudo systemctl disable openclaw-gateway 2>/dev/null || true

echo "=== 5. Suppression des fichiers systemd ==="
sudo rm -f /etc/systemd/system/openclaw.service
sudo rm -f /etc/systemd/system/openclaw-gateway.service
sudo rm -f /usr/lib/systemd/system/openclaw.service
sudo rm -f /usr/lib/systemd/system/openclaw-gateway.service
sudo systemctl daemon-reload

echo "=== 6. Suppression de l'installation globale OpenClaw ==="
# Si installé via npm
sudo npm uninstall -g openclaw 2>/dev/null || true
sudo npm uninstall -g moltbot 2>/dev/null || true
sudo npm uninstall -g clawdbot 2>/dev/null || true

# Suppression des binaires
sudo rm -f /usr/local/bin/openclaw
sudo rm -f /usr/local/bin/moltbot
sudo rm -f /usr/local/bin/clawdbot

echo "=== 7. Nettoyage des fichiers de configuration (ATTENTION: supprime tes configs!) ==="
# Sauvegarde d'abord si tu veux garder
# cp -r ~/.openclaw ~/.openclaw.backup

# Puis supprime
rm -rf ~/.openclaw
rm -rf ~/.moltbot
rm -rf ~/.clawdbot

echo "=== 8. Nettoyage des volumes Podman (optionnel) ==="
sudo podman volume prune -f

echo "=== 9. Vérification finale ==="
echo "Services systemd:"
systemctl list-units --all | grep -i openclaw || echo "  ✓ Aucun service OpenClaw"
echo ""
echo "Processus en cours:"
ps aux | grep -i openclaw | grep -v grep || echo "  ✓ Aucun processus OpenClaw"
echo ""
echo "Containers Podman:"
sudo podman ps -a | grep openclaw || echo "  ✓ Aucun container OpenClaw"
echo ""
echo "Images Podman:"
sudo podman images | grep openclaw || echo "  ✓ Aucune image OpenClaw"

echo ""
echo "✅ Nettoyage terminé!"
