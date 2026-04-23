wallbox:
mbpoll -m tcp -r 4 192.168.1.180 -1
mbpoll -m tcp -r 4 192.168.1.180 16


POWER:
mbpoll -m tcp -a 255 -t 4 -r 11027 -c 6 192.168.1.142
[11027]: 	0
[11028]: 	0
[11029]: 	0
[11030]: 	1850
[11031]: 	1000
[11032]: 	0


mbpoll -m tcp -a 255 -t 4 -r 11050 -c 30 192.168.1.142


SOC:
mbpoll -m tcp -a 255 -t 4 -r 11057 -c 1 192.168.1.142
[11057]: 	9900



All together:
mbpoll -m tcp -a 255 -t 4 -r 11003 -c 72 192.168.1.142


[11018]: 	569 - (/1000) Load

