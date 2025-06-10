TARGETS_FRONTEND ?= i686-pc-windows-gnu x86_64-pc-windows-gnu i686-unknown-linux-gnu x86_64-unknown-linux-gnu
TARGETS_BACKEND ?= i686-pc-windows-gnu x86_64-pc-windows-gnu i686-unknown-linux-gnu x86_64-unknown-linux-gnu
TARGETS_STANDALONE ?= i686-pc-windows-gnu x86_64-pc-windows-gnu i686-unknown-linux-gnu x86_64-unknown-linux-gnu
TARGETS_WIN7_BACKEND ?= x86_64-win7-windows-gnu

RELEASE_DIR:=release
DEBUG_DIR:=debug

BACKEND_RELEASE_BASE_RUST_FLAGS:=--remap-path-prefix ${HOME}=/foo -Zlocation-detail=none

BACKEND_RELEASE_LIB_RUST_FLAGS:=$(BACKEND_RELEASE_BASE_RUST_FLAGS)

BACKEND_RELEASE_BIN_RUST_FLAGS:=$(BACKEND_RELEASE_BASE_RUST_FLAGS)

BACKEND_BUILD_FLAGS:=-Z build-std=std,panic_abort -Z build-std-features=panic_immediate_abort

TOOLCHAIN_FRONTEND_DEBUG ?= stable
TOOLCHAIN_FRONTEND_RELEASE ?= stable
TOOLCHAIN_BACKEND_DEBUG ?= stable
TOOLCHAIN_BACKEND_RELEASE ?= nightly
TOOLCHAIN_STANDALONE_DEBUG ?= stable
TOOLCHAIN_STANDALONE_RELEASE ?= stable
TOOLCHAIN_WIN7_TAG ?= 1.87.0
TOOLCHAIN_WIN7_BACKEND ?= win7-$(TOOLCHAIN_WIN7_TAG)
TOOLCHAIN_WIN7_RUST_DIR = win7-rustc

SHELL:=bash

.PHONY: setup
setup:
	echo $(TOOLCHAIN_FRONTEND_DEBUG) $(TOOLCHAIN_FRONTEND_RELEASE) $(TOOLCHAIN_BACKEND_DEBUG) $(TOOLCHAIN_BACKEND_RELEASE) $(TOOLCHAIN_STANDALONE_DEBUG) $(TOOLCHAIN_STANDALONE_RELEASE) | tr ' ' '\n' | sort -u | while read toolchain ; do \
		rustup toolchain add $$toolchain || exit 1 ; \
	done
	@echo $(TARGETS_FRONTEND) $(TARGETS_BACKEND) $(TARGETS_STANDALONE) | tr ' ' '\n' | sort -u | while read target ; do \
		echo $(TOOLCHAIN_FRONTEND_DEBUG) $(TOOLCHAIN_FRONTEND_RELEASE) $(TOOLCHAIN_BACKEND_DEBUG) $(TOOLCHAIN_BACKEND_RELEASE) $(TOOLCHAIN_STANDALONE_DEBUG) $(TOOLCHAIN_STANDALONE_RELEASE) | tr ' ' '\n' | sort -u | while read toolchain ; do \
			echo ; echo "# Installing component $$target for $$toolchain" ; echo ; \
			rustup target add --toolchain $$toolchain $$target || exit 1 ; \
			if [[ ! "$$target" =~ "llvm" ]] ; then \
				rustup component add --toolchain $${toolchain}-$$target rust-src || exit 1 ; \
			fi ; \
		done ; \
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

.PHONY: win7
win7: build-win7
	@for t in $(TARGETS_WIN7_BACKEND) ; do \
		for f in backend/target/$$t/release/*soxy{,.dll,.exe,.so} ; do \
			if [[ -f "$$f" ]] ; then \
				mkdir -p $(RELEASE_DIR)/backend/$$t && \
				cp "$$f" $(RELEASE_DIR)/backend/$$t/ ; \
			fi ; \
		done ; \
	done

.PHONY: distclean
distclean: clean
	rm -rf ${RELEASE_DIR} ${DEBUG_DIR}
	$(MAKE) -C $(TOOLCHAIN_WIN7_RUST_DIR) $@

#############

.PHONY: build-release
build-release:
	@for t in $(TARGETS_FRONTEND) ; do \
		echo ; echo "# Building release frontend for $$t with $(TOOLCHAIN_FRONTEND_RELEASE)" ; echo ; \
		(cd frontend && cargo +$(TOOLCHAIN_FRONTEND_RELEASE) build --release --features log --target $$t && cd ..) || exit 1 ; \
	done
	@for t in $(TARGETS_BACKEND) ; do \
		echo ; echo "# Building release backend library for $$t with $(TOOLCHAIN_BACKEND_RELEASE)" ; echo ; \
		(cd backend && RUSTFLAGS="$(BACKEND_RELEASE_LIB_RUST_FLAGS)" cargo +$(TOOLCHAIN_BACKEND_RELEASE) build --lib --release --target $$t $(BACKEND_BUILD_FLAGS) && cd ..) || exit 1 ; \
		echo ; echo "# Building release backend binary for $$t with $(TOOLCHAIN_BACKEND_RELEASE)" ; echo ; \
		FLAGS="$(BACKEND_RELEASE_BIN_RUST_FLAGS)" ; \
		if echo $$t | grep -q windows ; then \
			FLAGS="$(BACKEND_RELEASE_BIN_RUST_FLAGS)" ; \
                fi ; \
		(cd backend && RUSTFLAGS="$$FLAGS" cargo +$(TOOLCHAIN_BACKEND_RELEASE) build --bins --release --target $$t $(BACKEND_BUILD_FLAGS) && cd ..) ; \
	done
	@for t in $(TARGETS_STANDALONE) ; do \
		echo ; echo "# Building release standalone for $$t with $(TOOLCHAIN_STANDALONE_RELEASE)" ; echo ; \
		(cd standalone && cargo +$(TOOLCHAIN_STANDALONE_RELEASE) build --release --features log --target $$t && cd ..) || exit 1 ; \
	done

.PHONY: build-debug
build-debug:
	@for t in $(TARGETS_FRONTEND) ; do \
		echo ; echo "# Building debug frontend for $$t with $(TOOLCHAIN_FRONTEND_DEBUG)" ; echo ; \
		(cd frontend && cargo +$(TOOLCHAIN_FRONTEND_DEBUG) build --features log --target $$t && cd ..) || exit 1 ; \
	done
	@for t in $(TARGETS_BACKEND) ; do \
		echo ; echo "# Building debug backend library for $$t with $(TOOLCHAIN_BACKEND_DEBUG)" ; echo ; \
		(cd backend && cargo +$(TOOLCHAIN_BACKEND_DEBUG) build --lib --features log --target $$t && cd ..) || exit 1 ; \
		echo ; echo "# Building debug backend binary for $$t with $(TOOLCHAIN_BACKEND_DEBUG)" ; echo ; \
		(cd backend && cargo +$(TOOLCHAIN_BACKEND_DEBUG) build --bins --features log --target $$t && cd ..) || exit 1 ; \
	done
	@for t in $(TARGETS_STANDALONE) ; do \
		echo ; echo "# Building debug standalone for $$t with $(TOOLCHAIN_STANDALONE_DEBUG)" ; echo ; \
		(cd standalone && cargo +$(TOOLCHAIN_STANDALONE_DEBUG) build --features log --target $$t && cd ..) || exit 1 ; \
	done

.PHONY: build-win7
build-win7:
	@echo "Checking the backend toolchain for a Win7 target"
	$(MAKE) -C $(TOOLCHAIN_WIN7_RUST_DIR)   \
		TAG=$(TOOLCHAIN_WIN7_TAG)           \
		TOOLCHAIN=$(TOOLCHAIN_WIN7_BACKEND) \
		TARGETS="$(TARGETS_WIN7_BACKEND)"
	@for t in $(TARGETS_WIN7_BACKEND) ; do  \
		echo ; echo "# Building release backend library for $$t with $(TOOLCHAIN_WIN7_BACKEND)" ; echo ; \
		(cd backend && \
		 RUSTFLAGS="$(BACKEND_RELEASE_LIB_RUST_FLAGS)" \
		 cargo +$(TOOLCHAIN_WIN7_BACKEND) build --lib --release --target $$t \
		) || exit 1 ; \
		echo ; echo "# Building release backend binary for $$t with $(TOOLCHAIN_WIN7_BACKEND)" ; echo ; \
		(cd backend && \
		 RUSTFLAGS="$(BACKEND_RELEASE_BIN_RUST_FLAGS)" \
		 cargo +$(TOOLCHAIN_WIN7_BACKEND) build --bins --release --target $$t \
		) ; \
	done

#############

.PHONY: clippy
clippy:
	@for t in $(TARGETS_FRONTEND) ; do \
		echo ; echo "# Clippy on frontend for $$t with $(TOOLCHAIN_FRONTEND_DEBUG)" ; echo ; \
		(cd frontend && cargo +$(TOOLCHAIN_FRONTEND_DEBUG) $@ --target $$t && cd ..) || exit 1 ; \
	done
	@for t in $(TARGETS_BACKEND) ; do \
		echo ; echo "# Clippy on backend for $$t with $(TOOLCHAIN_BACKEND_DEBUG)" ; echo ; \
		(cd backend && cargo +$(TOOLCHAIN_BACKEND_DEBUG) $@ --target $$t && cd ..) || exit 1 ; \
	done
	@for t in $(TARGETS_STANDALONE) ; do \
		echo ; echo "# Clippy on standalone for $$t with $(TOOLCHAIN_STANDALONE_DEBUG)" ; echo ; \
		(cd standalone && cargo +$(TOOLCHAIN_STANDALONE_DEBUG) $@ --target $$t && cd ..) || exit 1 ; \
	done

.PHONY: cargo-fmt
cargo-fmt:
	@for c in common frontend backend standalone ; do \
		(cd $$c && $@ +nightly && cd ..) || exit 1 ; \
	done

print-%:
	@echo $*=$($*)

%:
	@for c in common frontend backend standalone ; do \
		(cd $$c && cargo $@ && cd ..) || exit 1 ; \
	done
