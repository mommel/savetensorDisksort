## The problem that needs to be solved

Given

Multiple Storagedrives are running inside or on usb-c of a computer.
The OS of the computer is linux.
There are multiple tools using models, loras, embeddings and so on that are shuffeled over all of the 3,4 disks.
ln -s was the hero to have everything in place where it was needed.

But now i want a bit more order in that chaos.

So i need a new tool, that tool should be planed by you and afterwards it will be developed.

So what are the points that must be covered by the tool?

* I give a list on mountpoints / drivenames that should be lighted through.
* All real folder (no symlinks, neighter win nor linux) should be followed. Only the real folders count. To create a list with all .savetensors .pt and embeddings with it's real folder name
* walk the structure with symlinked stuff to get the usable treestructure
* After all is viewed and all lists are written a descrambling process should be planable (on what disk the structure should be created, and models moved into) with dryrun and finaly the real sort.
* The lists before the sort and the plan before the sort should have a exact Gb useage of each of the folders and subfolders.
* I would go with a json file for that planing but it's up to you what might be the best option.
* The sort thing should have a nice interface where i can check folder that should be sorted/created and then first copy than if checksum of both files same then delete the old one.

The language of the code is totally up to you. The tool itself should be robust and fast.

So please create that plan as `plans/planningstage.md` be detailed, be professional so that the development can be started on your plan afterwards, be precise, explain your decissions, mark pitfalls, and it would be super if you could plan it as tdd. With discreet testable steps to develop. But if that's too much skip the tdd thing.
