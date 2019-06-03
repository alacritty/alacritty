version := $(shell rpmspec -q --srpm --qf "%{version}\n" extra/linux/redhat/alacritty.spec)
commands = git

srpm: $(commands)
	$(eval top := $(shell mktemp -d))
	mkdir -p "$(top)/SOURCES"
	git archive HEAD --output "$(top)/SOURCES/alacritty-$(version).tar" --prefix "alacritty-$(version)/"
	rpmbuild -bs "$(spec)" --define "_topdir $(top)" --define "_srcrpmdir $(outdir)"
	rm -rf "$(top)"

$(commands):
	command -v $@ &> /dev/null || dnf -y install $@

.PHONY: srpm $(commands)
