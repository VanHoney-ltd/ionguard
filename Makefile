PREFIX ?= /usr/local
BINDIR = $(PREFIX)/bin
SHAREDIR = $(PREFIX)/share

.PHONY: all install uninstall

all:
	@echo "Run 'wails build' to build the application"

install:
	install -Dm755 build/bin/ionguard $(DESTDIR)$(BINDIR)/ionguard
	install -Dm755 build/bin/ionguard-core $(DESTDIR)$(BINDIR)/ionguard-core
	install -Dm644 packaging/ionguard.desktop $(DESTDIR)$(SHAREDIR)/applications/ionguard.desktop
	install -Dm644 packaging/ionguard.png $(DESTDIR)$(SHAREDIR)/pixmaps/ionguard.png
	install -Dm644 LICENSE $(DESTDIR)$(SHAREDIR)/licenses/ionguard/LICENSE

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/ionguard
	rm -f $(DESTDIR)$(BINDIR)/ionguard-core
	rm -f $(DESTDIR)$(SHAREDIR)/applications/ionguard.desktop
	rm -f $(DESTDIR)$(SHAREDIR)/pixmaps/ionguard.png
	rm -rf $(DESTDIR)$(SHAREDIR)/licenses/ionguard
