#!/bin/bash
echo "" > botlog.log
while read line; do
  echo "reading: ${line}" >> botlog.log
  echo "reading: ${line}"
done <&0
echo "stopping"
