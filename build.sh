# Example for Ubuntu/Debian-based Render environment
wget -q https://microsoft.com -O packages-microsoft-prod.deb && \
sudo dpkg -i packages-microsoft-prod.deb && \
sudo apt-get update && \
sudo apt-get install -y powershell && \
pwsh -NoProfile -ExecutionPolicy Bypass -File .\web\build.sh
