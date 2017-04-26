#!/usr/bin/env python3

import logging
log = logging.getLogger(__name__)

import collections
import shutil
import json
import sys
import os

import yaml

ALACONF_FN = os.path.expanduser('~/.config/alacritty/alacritty.yml')

Palette = collections.namedtuple('Pallete', ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'])


class AttrDict(dict):
    """
    >>> m = AttrDict(omg=True, whoa='yes')
    """

    def __init__(self, *args, **kwargs):
        super(AttrDict, self).__init__(*args, **kwargs)
        self.__dict__ = self


def slurp_yaml(fn):
    with open(fn, 'r') as fh:
        # JSON is a subset of YAML.
        contents = yaml.load(fh)
    return contents


def fixup_hex_color(*args):
    for arg in args:
        val = '0x%s' % arg.strip('#')
        yield val


def convert(tilix_scheme):
    j = AttrDict(tilix_scheme)
    palette = list(fixup_hex_color(*j.palette))

    pal_normal = Palette(*palette[:8])
    pal_bold = Palette(*palette[8:])

    colors = {
        'primary': dict(zip(
            ['background', 'foreground'],
            fixup_hex_color(j['background-color'], j['foreground-color']),
        )),
        'cursor': dict(zip(
            ['text', 'cursor'],
            fixup_hex_color(j['cursor-background-color'], j['cursor-foreground-color']),
        )),
        'normal': dict(pal_normal._asdict()),
        'bright': dict(pal_bold._asdict()),
    }

    return colors


def patch_alaconf_colors(colors, alaconf_fn=ALACONF_FN):
    with open(alaconf_fn, 'r') as fh:
        ac_raw = fh.read()

    # Write config file taking care to not remove delicious comments.
    # Sure, it's janky, but less so than losing comments.
    skipping = False
    lines = []
    for line in ac_raw.splitlines():
        if skipping:
            if line and line[0].isalpha():
                skipping = False

        elif line.startswith('colors:'):
            skipping = True

        if not skipping:
            if not line and lines and not lines[-1]:
                continue
            lines.append(line)

    temp_fn = '%s.tmp' % alaconf_fn
    backup_fn = '%s.bak' % alaconf_fn

    with open(temp_fn, 'w') as fh:
        fh.write('\n'.join(lines))
        fh.write('\n')
        yaml.safe_dump(dict(colors=colors), fh)

    shutil.copyfile(alaconf_fn, backup_fn)
    os.rename(temp_fn, alaconf_fn)


def main(argv=sys.argv):
    if len(argv) != 2:
        print("Usage: %s TILIX_SCHEME_JSON_FILE" % sys.executable, file=sys.stderr)
        sys.exit(1)

    fn = argv[1]

    tilix_scheme = slurp_yaml(fn)
    colors = convert(tilix_scheme)
    patch_alaconf_colors(colors)


if __name__ == '__main__':
    main()
