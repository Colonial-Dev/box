set -eu

buildah() {
    if [ "$1" = 'from' ]; then
        ctr=$(command buildah "$@")
        
        buildah config \
            -a manager=box \
            -a box.path=$__BOX_BUILD_PATH \
            -a box.hash=$__BOX_BUILD_HASH \
            -a box.tree=$__BOX_BUILD_TREE \
            -a box.name=$__BOX_BUILD_NAME \
            "$ctr"

        buildah config -a \
            box.instantiate=$__BOX_BUILD_INSTANTIATE \
            "$ctr"

        export __BOX_BUILD_CTR="$ctr"
    else
        command buildah "$@"
    fi
}

FROM() {
    buildah from $@
}

COMMIT() {
    bx config commit $@
}

RUN() {
    bx config run $@
}

ADD() {
    bx config add $@
}

COPY() {
    ADD $@
}

CMD() {
    buildah config --cmd "$@" $__BOX_BUILD_CTR
}

LABEL() {
    buildah config --label "$@" $__BOX_BUILD_CTR
}

EXPOSE() {
    buildah config --port "$@" $__BOX_BUILD_CTR
}

ENV() {
    buildah config --env "$@" $__BOX_BUILD_CTR
}

ENTRYPOINT() {
    buildah config --entrypoint "$@" $__BOX_BUILD_CTR
}

VOLUME() {
    buildah config --volume "$@" $__BOX_BUILD_CTR
}

USER() {
    buildah config --user "$@" $__BOX_BUILD_CTR
}

WORKDIR() {
    buildah config --workingdir "$@" $__BOX_BUILD_CTR
}

SHELL() {
    buildah config --shell "$@" $__BOX_BUILD_CTR
}

HEALTHCHECK() {
    buildah config --healthcheck "$@" $__BOX_BUILD_CTR
}

STOPSIGNAL() {
    buildah config --stop-signal "$@" $__BOX_BUILD_CTR
}

CFG() {
    bx config $@
}

PRESET() {
    bx config preset $@
}

cd $__BOX_BUILD_DIR
