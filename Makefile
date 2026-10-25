# Every target trims target/ afterwards (even on failure) so builds can't quietly
# eat the disk. Tune with TARGET_CAP_MB and TARGET_STALE_DAYS.

CARGO ?= cargo
TRIM  := ./scripts/trim-target.sh

.PHONY: run release build test check clippy fmt trim clean

run:
	@$(CARGO) run -p bivouac-client; s=$$?; $(TRIM); exit $$s

release:
	@$(CARGO) run --release -p bivouac-client; s=$$?; $(TRIM); exit $$s

build:
	@$(CARGO) build; s=$$?; $(TRIM); exit $$s

test:
	@$(CARGO) test; s=$$?; $(TRIM); exit $$s

check:
	@$(CARGO) check --all-targets; s=$$?; $(TRIM); exit $$s

clippy:
	@$(CARGO) clippy --all-targets; s=$$?; $(TRIM); exit $$s

fmt:
	@$(CARGO) fmt

trim:
	@$(TRIM)

clean:
	@$(CARGO) clean
