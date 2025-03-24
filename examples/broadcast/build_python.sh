rm -rf build
mkdir -p build
chmod 777 build

docker run                                        \
        --platform linux/x86_64                   \
        -u $(id -u):$(id -g)                      \
        -v $(pwd)/examples/broadcast:/data          \
        -v $(pwd)/build:/build                    \
        -w /                                      \
        swirlc-rust                               \
    swirlc                                        \
        compile                                   \
        --target default                             \
        /data/source.swirl                      \
        /data/config.yml

# create the folder on the server
ssh tommaso.fogliobonda@c3sfr1.di.unito.it 'mkdir -p ~/swirlc-broadcast/python'
ssh tommaso.fogliobonda@c3sfr1.di.unito.it 'mkdir -p ~/swirlc-broadcast/python/src'

cd ./build
# zip the target/release/location* files (ignore locaition*.d files)
zip -r location.zip *.py run.sh
# send the zip file to the server
scp location.zip tommaso.fogliobonda@c3sfr1.di.unito.it:~/swirlc-broadcast/python/location.zip
# remove the zip file
rm location.zip


# remove the previous files
ssh tommaso.fogliobonda@c3sfr1.di.unito.it 'rm -rf ~/swirlc-broadcast/python/src/*.py && rm -rf ~/swirlc-broadcast/python/src/run.sh'

# unzip the zip file on the server
ssh tommaso.fogliobonda@c3sfr1.di.unito.it 'unzip -o ~/swirlc-broadcast/python/location.zip -d ~/swirlc-broadcast/python/src && rm ~/swirlc-broadcast/python/location.zip'