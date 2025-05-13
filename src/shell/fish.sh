function trap -a command
    function $command -V command
        command $command $argv
        
        set -l s $status

        if [ $s -ne 0 ]
            exit $s
        else
            return 0
        end
    end
end

trap bx

function buildah
    if [ $argv[1] = 'from' ]
        set -l ctr (command buildah $argv)

        if [ $status -ne 0 ]
            exit $status
        end
        
        buildah config \
            -a manager=box \
            -a box.path=$__BOX_BUILD_PATH \
            -a box.hash=$__BOX_BUILD_HASH \
            -a box.tree=$__BOX_BUILD_TREE \
            -a box.name=$__BOX_BUILD_NAME \
            $ctr

        buildah config -a \
            box.instantiate=$__BOX_BUILD_INSTANTIATE \
            $ctr

        set -gx __BOX_BUILD_CTR $ctr
    else
        command buildah $argv

        if [ $status -ne 0 ]
            exit $status
        end
    end
end

function FROM
    buildah from $argv
end

function COMMIT
    bx config commit $argv
end

function RUN
    bx config run $argv
end

function ADD
    bx config add $argv
end

function COPY
    ADD $argv
end

function CMD
    buildah config --cmd "$argv" $__BOX_BUILD_CTR
end

function LABEL
    buildah config --label "$argv" $__BOX_BUILD_CTR
end

function EXPOSE
    buildah config --port "$argv" $__BOX_BUILD_CTR
end

function ENV
    buildah config --env "$argv" $__BOX_BUILD_CTR
end

function ENTRYPOINT
    buildah config --entrypoint "$argv" $__BOX_BUILD_CTR
end

function VOLUME
    buildah config --volume "$argv" $__BOX_BUILD_CTR
end

function USER
    buildah config --user "$argv" $__BOX_BUILD_CTR
end

function WORKDIR
    buildah config --workingdir "$argv" $__BOX_BUILD_CTR
end

function SHELL
    buildah config --shell "$argv" $__BOX_BUILD_CTR
end

function HEALTHCHECK
    buildah config --healthcheck "$argv" $__BOX_BUILD_CTR
end

function STOPSIGNAL
    buildah config --stop-signal "$argv" $__BOX_BUILD_CTR
end

function CFG
    bx config $argv
end

function PRESET
    bx config preset $argv
end

cd $__BOX_BUILD_DIR
