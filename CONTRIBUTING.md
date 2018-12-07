## Contributing to Alacritty

Looks like you are interested in contributing to alacritty, thank you for your time.The following are just guidelines except for the code formatting which should be adhered to. So if you know something that can be done in fewer commands, use that or
even better submit a PR.

This document will introduce some things that will get you started with 
- Setting up local development environment.
- Formatting Code before PR's 

## Setting up local development environment.

1. Clone a copy of Alacritty from source 

    ``` git clone --origin upstream https://github.com/jwilm/alacritty.git ```

   The above command does two things for you, it clones the repository into your local directory
   and it checksout the master branch. It sets the remote name as upstream because this branch is
   used to track changes that are made on the main repository.If you're new to git don't worry the 
   jargon gets clear as you follow along.

2. Got to alacritty's main github repository and fork it. It's on the top right corner.

3. Now you have your own copy of the repository. Now copy the repository link of your forked repository.
   It can be found on the top right corner saying clone or download.

   ``` 
   cd alacritty
   
   git remote add fork https://github.com/yourusername/alacritty.git
   ```
4. Now the issue you want to tackle will have a number, go to that issue remember the number.
   Use the following commands in your shell.
   ```
   git checkout -b FIX-#issue_number
   ```
5. Make the required changes,in your working directory there will be file called changelog add an 
   entry to it about the issue you are trying to fix.Keep it short and concise, do not end them with
   a period or a punctuation mark. If your happy with the changes you've made commit them using
   ```
    git add changed_files
    git commit 
   ```
   Guidelines for writing good commit messages can be found.[here](https://tbaggery.com/2008/04/19/a-note-about-git-commit-messages.html).

6. Now before pushing,you'll have to be sure that some changes made on the main repository while you were working on the patch 
   do not create merge conflicts,so use the following commands before you push so that it'll be easier for the maintainers to
   merge your PR into the main branch.
   ```
   git checkout master
   git pull
   git checkout FIX-#issue_number
   git rebase master #if there are merge conflicts git will shout at you which is ok,just resolve them,the error messages(if they are conflicts)
   will be very informative just follow them.

   git push -u fork HEAD:FIX-#issue_number #if you're pushing for the first time, if you have done this just use git push and git will know where to send changes
   ```
7. Now go to your forked repository and Open a [PR](https://help.github.com/articles/creating-a-pull-request/).

8. Once you have made a PR, the maintainer will tell you if there are changes to be made. Go to step 6 and follow the same rules, instead of the push command use
   ```
   #if the changes are merely formatting/style issues use git commit --amend instead of git commit.
   git push -f
   ```
The following 6-8 works without any side-effects if you're working alone. Do not ammend commits if you're working with a different person, the rebase works just fine but make sure that they know you have rebased.

## Formatting Code before PR's Style

To be added.