# Image: tapis/events-engine-ex-pyplugin
FROM tapis/pyevents

ADD plugin.py /plugin.py

ENTRYPOINT [ "python" ]
# use unbuffered output so logs show up instantly
CMD [ "-u", "/plugin.py" ]