TARGETS_FRONTEND ?= i686-pc-windows-gnu x86_64-pc-windows-gnu i686-unknown-linux-gnu x86_64-unknown-linux-gnu
TARGETS_BACKEND ?= i686-pc-windows-gnu x86_64-pc-windows-gnu i686-unknown-linux-gnu x86_64-unknown-linux-gnu
TARGETS_STANDALONE ?= i686-pc-windows-gnu x86_64-pc-windows-gnu i686-unknown-linux-gnu x86_64-unknown-linux-gnu

RELEASE_DIR:=release
DEBUG_DIR:=debug

BACKEND_RELEASE_LIB_RUST_FLAGS=--remap-path-prefix ${HOME}=/foo -Zlocation-detail=none

BACKEND_RELEASE_BIN_RUST_FLAGS=--remap-path-prefix ${HOME}=/foo -Zlocation-detail=none
BACKEND_RELEASE_BIN_WINDOWS_RUST_FLAGS=$(BACKEND_RELEASE_BIN_RUST_FLAGS) -Ctarget-feature=+crt-static

BACKEND_BUILD_FLAGS=-Z build-std=std,panic_abort -Z build-std-features=panic_immediate_abort

SHELL:=bash

.PHONY: setup
setup:
	rustup toolchain add stable
	rustup toolchain add nightly
	echo $(TARGETS_FRONTEND) $(TARGETS_BACKEND) $(TARGETS_STANDALONE) | tr ' ' '\n' | sort -u | while read t ; do \
		echo ; echo "# Installing toolchains and components for $$t" ; echo ; \
		rustup target add --toolchain stable $$t ; \
		rustup target add --toolchain nightly $$t ; \
		rustup component add --toolchain nightly-$$t rust-src ; \
	done

.PHONY: release
release: build-release
	@for t in $(TARGETS_FRONTEND) ; do \
		for f in frontend/target/$$t/release/*soxy{,.dll,.exe,.so} ; do \
			if [[ -f "$$f" ]] ; then \
				mkdir -p $(RELEASE_DIR)/frontend/$$t && \
				cp "$$f" $(RELEASE_DIR)/frontend/$$t/ ; \
			fi ; \
		done ; \
	done
	@for t in $(TARGETS_BACKEND) ; do \
		for f in backend/target/$$t/release/*soxy{,.dll,.exe,.so} ; do \
			if [[ -f "$$f" ]] ; then \
				mkdir -p $(RELEASE_DIR)/backend/$$t && \
				cp "$$f" $(RELEASE_DIR)/backend/$$t/ ; \
			fi ; \
		done ; \
	done
	@for t in $(TARGETS_STANDALONE) ; do \
		for f in standalone/target/$$t/release/*standalone{,.exe} ; do \
			if [[ -f "$$f" ]] ; then \
				mkdir -p $(RELEASE_DIR)/standalone/$$t && \
				cp "$$f" $(RELEASE_DIR)/standalone/$$t/ ; \
			fi ; \
		done ; \
	done

.PHONY: debug
debug: build-debug
	@for t in $(TARGETS_FRONTEND) ; do \
		for f in frontend/target/$$t/debug/*soxy{,.dll,.exe,.so} ; do \
			if [[ -f "$$f" ]] ; then \
				mkdir -p $(DEBUG_DIR)/frontend/$$t && \
				cp "$$f" $(DEBUG_DIR)/frontend/$$t/ ; \
			fi ; \
		done ; \
	done
	@for t in $(TARGETS_BACKEND) ; do \
		for f in backend/target/$$t/debug/*soxy{,.dll,.exe,.so} ; do \
			if [[ -f "$$f" ]] ; then \
				mkdir -p $(DEBUG_DIR)/backend/$$t && \
				cp "$$f" $(DEBUG_DIR)/backend/$$t/ ; \
			fi ; \
		done ; \
	done
	@for t in $(TARGETS_STANDALONE) ; do \
		for f in standalone/target/$$t/debug/*standalone{,.exe} ; do \
			if [[ -f "$$f" ]] ; then \
				mkdir -p $(DEBUG_DIR)/standalone/$$t && \
				cp "$$f" $(DEBUG_DIR)/standalone/$$t/ ; \
			fi ; \
		done ; \
	done

.PHONY: distclean
distclean: clean
	rm -rf ${RELEASE_DIR} ${DEBUG_DIR}

#############

.PHONY: build-release
build-release:
	@for t in $(TARGETS_FRONTEND) ; do \
		echo ; echo "# Building release frontend for $$t" ; echo ; \
		cd frontend ; cargo build --release --features log --target $$t ; cd .. ; \
	done
	@for t in $(TARGETS_BACKEND) ; do \
		echo ; echo "# Building release backend library for $$t" ; echo ; \
		cd backend ; RUSTFLAGS="$(BACKEND_RELEASE_LIB_RUST_FLAGS)" cargo +nightly build --lib --release --target $$t $(BACKEND_BUILD_FLAGS) ; cd .. ; \
		echo ; echo "# Building release backend binary for $$t" ; echo ; \
		FLAGS="$(BACKEND_RELEASE_BIN_RUST_FLAGS)" ; \
		if echo $$t | grep -q windows ; then \
			FLAGS="$(BACKEND_RELEASE_BIN_WINDOWS_RUST_FLAGS)" ; \
                fi ; \
		cd backend ; RUSTFLAGS="$$FLAGS" cargo +nightly build --bins --release --target $$t $(BACKEND_BUILD_FLAGS) ; cd .. ; \
	done
	@for t in $(TARGETS_STANDALONE) ; do \
		echo ; echo "# Building release standalone for $$t" ; echo ; \
		cd standalone ; cargo build --release --features log --target $$t ; cd .. ; \
	done

.PHONY: build-debug
build-debug:
	@for t in $(TARGETS_FRONTEND) ; do \
		echo ; echo "# Building debug frontend for $$t" ; echo ; \
		cd frontend ; cargo build --features log --target $$t ; cd .. ; \
	done
	@for t in $(TARGETS_BACKEND) ; do \
		echo ; echo "# Building debug backend library for $$t" ; echo ; \
		cd backend ; cargo build --lib --features log --target $$t ; cd .. ; \
		echo ; echo "# Building debug backend binary for $$t" ; echo ; \
		cd backend ; cargo build --bins --features log --target $$t ; cd .. ; \
	done
	@for t in $(TARGETS_STANDALONE) ; do \
		echo ; echo "# Building debug standalone for $$t" ; echo ; \
		cd standalone ; cargo build --features log --target $$t ; cd .. ; \
	done

#############

.PHONY: clippy
clippy:
	@for t in i686-pc-windows-gnu x86_64-pc-windows-gnu i686-unknown-linux-gnu x86_64-unknown-linux-gnu ; do \
		for c in common frontend backend standalone ; do \
			echo ; echo "# Clippy on $$c for $$t" ; echo ; \
			cd $$c ; cargo $@ --target $$t ; cd .. ; \
		done ; \
	done

.PHONY: cargo-fmt
cargo-fmt:
	for c in common frontend backend standalone ; do \
		cd $$c ; $@ ; cd .. ; \
	done

%:
	for c in common frontend backend standalone ; do \
		cd $$c ; cargo $@ ; cd .. ; \
	done
