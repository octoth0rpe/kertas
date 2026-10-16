.PHONY: install

# Builds a release Kertas.app, quits the installed copy if it's running, and
# replaces it in ~/Applications. See scripts/bundle-macos.sh.
install:
	scripts/bundle-macos.sh --install
